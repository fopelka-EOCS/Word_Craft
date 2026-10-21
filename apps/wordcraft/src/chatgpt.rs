//! Native-only ChatGPT OAuth. Secrets live in this worker and macOS Keychain, never UI settings.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use url::Url;
use wordcraft_assistant::{
    Http, HttpRequest, Method,
    chatgpt::{Connection, Status},
    responses,
};

const ISSUER: &str = "https://auth.openai.com";
const TOKEN: &str = "https://auth.openai.com/api/accounts/oauth/token";
const JWKS: &str = "https://auth.openai.com/.well-known/jwks.json";
const SERVICE: &str = "ai.storyteller.wordcraft.chatgpt.local-prototype";
const STORAGE_ERROR: &str = "macOS Keychain unavailable. No credentials were stored in a file. Unlock Keychain and try again.";
const AUTH_ERROR: &str = "ChatGPT authorization could not be verified. Continue with ChatGPT to sign in again.";
const CANCELLED: &str = "ChatGPT operation cancelled.";
const MAX_BODY: u64 = 8 * 1024 * 1024;

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}
fn random() -> Result<String, String> {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).map_err(|_| "Secure randomness unavailable.")?;
    Ok(URL_SAFE_NO_PAD.encode(b))
}
fn host_id() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| "Secure randomness unavailable.")?;
    Ok(format!("urn:uuid:{}", uuid::Builder::from_random_bytes(bytes).into_uuid()))
}

fn client() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(Duration::from_secs(180)))
        .build()
        .into()
}
fn fetch_json(url: &str) -> Result<Value, String> {
    let mut r = client().get(url).call().map_err(|_| AUTH_ERROR)?;
    if !r.status().is_success() {
        return Err(AUTH_ERROR.into());
    }
    let s = r.body_mut().with_config().limit(MAX_BODY).read_to_string().map_err(|_| AUTH_ERROR)?;
    serde_json::from_str(&s).map_err(|_| AUTH_ERROR.into())
}
fn form(url: &str, fields: &[(&str, &str)]) -> Result<Value, String> {
    let body = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(fields.iter().copied()).finish();
    let mut r = client().post(url).header("content-type", "application/x-www-form-urlencoded").send(&body).map_err(|_| AUTH_ERROR)?;
    if !r.status().is_success() {
        return Err(AUTH_ERROR.into());
    }
    let s = r.body_mut().with_config().limit(MAX_BODY).read_to_string().map_err(|_| AUTH_ERROR)?;
    if s.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&s).map_err(|_| AUTH_ERROR.into())
}

trait Store: Send {
    fn load(&self, name: &str) -> Result<Option<Vec<u8>>, String>;
    fn save(&self, name: &str, bytes: &[u8]) -> Result<(), String>;
    fn remove(&self, name: &str) -> Result<(), String>;
}
struct Keychain;
impl Store for Keychain {
    fn load(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        match security_framework::passwords::get_generic_password(SERVICE, name) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.code() == -25300 => Ok(None),
            Err(_) => Err(STORAGE_ERROR.into()),
        }
    }
    fn save(&self, name: &str, b: &[u8]) -> Result<(), String> {
        security_framework::passwords::set_generic_password(SERVICE, name, b).map_err(|_| STORAGE_ERROR.into())
    }
    fn remove(&self, name: &str) -> Result<(), String> {
        match security_framework::passwords::delete_generic_password(SERVICE, name) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == -25300 => Ok(()),
            Err(_) => Err(STORAGE_ERROR.into()),
        }
    }
}
fn save(store: &dyn Store, name: &str, v: &impl Serialize) -> Result<(), String> {
    let b = serde_json::to_vec(v).map_err(|_| STORAGE_ERROR)?;
    store.save(name, &b)
}
fn load<T: for<'de> Deserialize<'de>>(store: &dyn Store, name: &str) -> Result<Option<T>, String> {
    store.load(name)?.map(|b| serde_json::from_slice(&b).map_err(|_| STORAGE_ERROR.into())).transpose()
}

#[derive(Clone, Serialize, Deserialize)]
struct Registration {
    host: String,
    client_id: String,
    subject: String,
    email: Option<String>,
    acknowledged: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct Credentials {
    access_token: String,
    refresh_token: String,
    id_token: String,
    scope: String,
    expires_at: u64,
    earliest_refresh_at: u64,
}
struct Runtime {
    store: Box<dyn Store>,
    registration: Registration,
    credentials: Option<Credentials>,
    // The lock protects rotating tokens from a second WordCraft process using this same service.
    _lock: Option<std::fs::File>,
}
impl Runtime {
    fn load(store: Box<dyn Store>, lock: Option<std::fs::File>) -> Result<Self, String> {
        let mut registration = match load(store.as_ref(), "registration")? {
            Some(r) => r,
            None => {
                let r = Registration { host: host_id()?, client_id: String::new(), subject: String::new(), email: None, acknowledged: false };
                save(store.as_ref(), "registration", &r)?;
                r
            }
        };
        let saved: Option<(Registration, Credentials)> = load(store.as_ref(), "credentials")?;
        let credentials = saved.map(|(r, c)| {
            registration = r;
            c
        });
        Ok(Self { store, registration, credentials, _lock: lock })
    }
    fn status(&self, generation: u64) -> Status {
        Status {
            connected: self.credentials.is_some(),
            account: if self.credentials.is_some() { self.registration.email.clone().or_else(|| Some("ChatGPT account".into())) } else { None },
            usage_acknowledged: self.registration.acknowledged,
            generation,
            ..Default::default()
        }
    }
    fn remember(&mut self, registration: Registration, credentials: Credentials) -> Result<(), String> {
        // One Keychain update binds the complete token set to its verified registration.
        save(self.store.as_ref(), "credentials", &(&registration, &credentials))?;
        self.registration = registration;
        self.credentials = Some(credentials);
        Ok(())
    }
    fn refresh(&mut self, exchange: impl FnOnce(&Registration, &Credentials) -> Result<Value, String>) -> Result<(), String> {
        let c = self.credentials.as_ref().ok_or(AUTH_ERROR)?;
        if now().saturating_add(60) < c.expires_at {
            return Ok(());
        }
        if now() < c.earliest_refresh_at {
            return Err("ChatGPT authorization is waiting for its permitted refresh time. Try again later.".into());
        }
        let result = exchange(&self.registration, c).and_then(|v| tokens(&v, Some(c)));
        match result {
            Ok(c) => self.remember(self.registration.clone(), c),
            Err(e) => {
                self.forget()?;
                Err(e)
            }
        }
    }
    fn forget(&mut self) -> Result<(), String> {
        // Prepare metadata first so a successful deletion can always proceed to revocation.
        save(self.store.as_ref(), "registration", &self.registration)
            .map_err(|_| "Could not prepare ChatGPT sign-out in Keychain. Disconnect again before closing WordCraft.")?;
        self.store
            .remove("credentials")
            .map_err(|_| "Could not remove ChatGPT credentials from Keychain. Disconnect again before closing WordCraft.")?;
        self.credentials = None;
        Ok(())
    }
}
fn tokens(v: &Value, previous: Option<&Credentials>) -> Result<Credentials, String> {
    let field = |k| v.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string).ok_or(AUTH_ERROR);
    if !field("token_type")?.eq_ignore_ascii_case("bearer") {
        return Err(AUTH_ERROR.into());
    }
    let scope = v.get("scope").and_then(Value::as_str).map(str::to_string).or_else(|| previous.map(|c| c.scope.clone())).ok_or(AUTH_ERROR)?;
    if !scope.split_whitespace().any(|s| s == "chatgpt.tokens.use.direct") {
        return Err("ChatGPT plan use was not authorized. Enable it in ChatGPT settings and sign in again.".into());
    }
    let expires = v.get("expires_in").and_then(Value::as_u64).filter(|n| *n > 0 && *n <= 86400).ok_or(AUTH_ERROR)?;
    Ok(Credentials {
        access_token: field("access_token")?,
        refresh_token: field("refresh_token")?,
        id_token: v.get("id_token").and_then(Value::as_str).map(str::to_string).or_else(|| previous.map(|c| c.id_token.clone())).ok_or(AUTH_ERROR)?,
        scope,
        expires_at: now().saturating_add(expires),
        earliest_refresh_at: v.get("earliest_refresh_at").and_then(Value::as_u64).unwrap_or(0),
    })
}
#[derive(Clone, Deserialize)]
struct Identity {
    sub: String,
    nonce: Option<String>,
    email: Option<String>,
}
fn identity(token: &str, client_id: &str, nonce: &str, keys: &Value) -> Result<Identity, String> {
    use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
    let header = decode_header(token).map_err(|_| AUTH_ERROR)?;
    if !matches!(header.alg, Algorithm::RS256 | Algorithm::ES256) {
        return Err(AUTH_ERROR.into());
    }
    let keys: JwkSet = serde_json::from_value(keys.clone()).map_err(|_| AUTH_ERROR)?;
    let key = keys.find(header.kid.as_deref().ok_or(AUTH_ERROR)?).ok_or(AUTH_ERROR)?;
    let key = DecodingKey::from_jwk(key).map_err(|_| AUTH_ERROR)?;
    let mut validation = Validation::new(header.alg);
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[client_id]);
    validation.set_required_spec_claims(&["sub", "exp", "iat", "iss", "aud"]);
    validation.leeway = 5;
    validation.validate_nbf = true;
    let id = decode::<Identity>(token, &key, &validation).map_err(|_| AUTH_ERROR)?.claims;
    if id.sub.is_empty() || id.nonce.as_deref() != Some(nonce) {
        return Err(AUTH_ERROR.into());
    }
    Ok(id)
}

struct Attempt {
    state: String,
    nonce: String,
    verifier: String,
    redirect: String,
    client_id: String,
}
impl Attempt {
    fn new(port: u16, registration: &Registration) -> Result<Self, String> {
        Ok(Self {
            state: random()?,
            nonce: random()?,
            verifier: random()?,
            redirect: format!("http://127.0.0.1:{port}/auth/callback"),
            client_id: registration.client_id.clone(),
        })
    }
    fn url(&self, r: &Registration, c: Option<&Credentials>) -> Result<Url, String> {
        let mut u = Url::parse("https://auth.openai.com/api/accounts/authorize").map_err(|_| AUTH_ERROR)?;
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(self.verifier.as_bytes()));
        {
            let mut q = u.query_pairs_mut();
            q.extend_pairs([
                ("client_id", if self.client_id.is_empty() { "dynamic_agent_client" } else { &self.client_id }),
                ("ext_agent_host_id", r.host.as_str()),
                ("response_type", "code"),
                ("redirect_uri", &self.redirect),
                ("scope", "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct"),
                ("resource", responses::BASE),
                ("state", &self.state),
                ("nonce", &self.nonce),
                ("code_challenge_method", "S256"),
                ("code_challenge", &challenge),
            ]);
            if self.client_id.is_empty() {
                q.append_pair("agent_name_hint", "WordCraft");
            } else if let Some(c) = c {
                q.append_pair("id_token_hint", &c.id_token);
            }
        }
        Ok(u)
    }
    fn callback(&self, target: &str) -> Result<(String, String), String> {
        let u = Url::parse(&format!("http://127.0.0.1{target}")).map_err(|_| AUTH_ERROR)?;
        if u.path() != "/auth/callback" || u.fragment().is_some() {
            return Err(AUTH_ERROR.into());
        }
        let mut fields = std::collections::BTreeMap::new();
        for (k, v) in u.query_pairs() {
            if fields.insert(k.to_string(), v.to_string()).is_some() {
                return Err(AUTH_ERROR.into());
            }
        }
        if fields.get("state") != Some(&self.state) {
            return Err(AUTH_ERROR.into());
        }
        if fields.contains_key("error") {
            return Err("ChatGPT sign-in was declined or cancelled.".into());
        }
        let code = fields.get("code").filter(|c| !c.is_empty()).ok_or(AUTH_ERROR)?.clone();
        let id = fields.get("client_id").cloned().unwrap_or_else(|| self.client_id.clone());
        if !id.starts_with("oaiapp_") || (!self.client_id.is_empty() && id != self.client_id) {
            return Err(AUTH_ERROR.into());
        }
        Ok((code, id))
    }
}
#[derive(Clone)]
struct Ticket {
    epoch: Arc<AtomicU64>,
    value: u64,
}
impl Ticket {
    fn check(&self) -> Result<(), String> {
        if self.epoch.load(Ordering::SeqCst) == self.value { Ok(()) } else { Err(CANCELLED.into()) }
    }
}
fn callback(listener: &TcpListener, a: &Attempt, t: &Ticket, timeout: Duration) -> Result<(String, String), String> {
    listener.set_nonblocking(true).map_err(|_| AUTH_ERROR)?;
    let start = Instant::now();
    loop {
        t.check()?;
        if start.elapsed() >= timeout {
            return Err("ChatGPT sign-in timed out. Continue with ChatGPT to try again.".into());
        }
        match listener.accept() {
            Ok((mut s, _)) => {
                // Accepted sockets can inherit nonblocking mode on macOS.
                s.set_nonblocking(false).map_err(|_| AUTH_ERROR)?;
                s.set_read_timeout(Some(Duration::from_millis(40))).map_err(|_| AUTH_ERROR)?;
                s.set_write_timeout(Some(Duration::from_millis(300))).map_err(|_| AUTH_ERROR)?;
                let mut reader = BufReader::new(&s);
                let mut line = Vec::new();
                while line.len() <= 8192 {
                    t.check()?;
                    if start.elapsed() >= timeout {
                        return Err("ChatGPT sign-in timed out. Continue with ChatGPT to try again.".into());
                    }
                    let mut byte = [0];
                    match reader.read(&mut byte) {
                        Ok(0) => break,
                        Ok(_) => {
                            line.push(byte[0]);
                            if byte[0] == b'\n' {
                                break;
                            }
                        }
                        Err(e)
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted
                            ) => {}
                        Err(_) => break,
                    }
                }
                if line.len() > 8192 || !line.ends_with(b"\n") {
                    continue;
                }
                let Ok(line) = std::str::from_utf8(&line) else { continue };
                let parts: Vec<_> = line.split_whitespace().collect();
                if parts.first() != Some(&"GET") {
                    continue;
                }
                let Some(target) = parts.get(1).filter(|t| t.starts_with("/auth/callback?")) else { continue };
                let result = a.callback(target);
                let text =
                    if result.is_ok() { "Return to WordCraft to finish connecting." } else { "Sign-in could not be verified. Return to WordCraft." };
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{text}",
                    text.len()
                );
                t.check()?;
                return result;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(40)),
            Err(_) => return Err(AUTH_ERROR.into()),
        }
    }
}
fn sign_in(rt: &mut Runtime, t: &Ticket) -> Result<(), String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| "Could not start the local ChatGPT sign-in callback.")?;
    let a = Attempt::new(listener.local_addr().map_err(|_| AUTH_ERROR)?.port(), &rt.registration)?;
    let url = a.url(&rt.registration, rt.credentials.as_ref())?;
    t.check()?;
    open::that_detached(url.as_str()).map_err(|_| "Could not open the system browser for ChatGPT sign-in.")?;
    let (code, id) = callback(&listener, &a, t, Duration::from_secs(300))?;
    complete_sign_in(
        rt,
        &a,
        &id,
        t,
        || {
            form(
                TOKEN,
                &[
                    ("grant_type", "authorization_code"),
                    ("client_id", &id),
                    ("code", &code),
                    ("code_verifier", &a.verifier),
                    ("redirect_uri", &a.redirect),
                    ("resource", responses::BASE),
                ],
            )
        },
        || fetch_json(JWKS),
    )
}
fn complete_sign_in(
    rt: &mut Runtime,
    a: &Attempt,
    id: &str,
    t: &Ticket,
    exchange: impl FnOnce() -> Result<Value, String>,
    keys: impl FnOnce() -> Result<Value, String>,
) -> Result<(), String> {
    t.check()?;
    let v = exchange()?;
    t.check()?;
    let c = tokens(&v, None)?;
    let identity = identity(&c.id_token, id, &a.nonce, &keys()?)?;
    if !rt.registration.subject.is_empty() && identity.sub != rt.registration.subject {
        return Err("This prototype supports one registered ChatGPT account. Sign in with the account originally connected to WordCraft.".into());
    }
    let r = Registration {
        host: rt.registration.host.clone(),
        client_id: id.into(),
        subject: identity.sub,
        email: identity.email.filter(|e| !e.trim().is_empty() && e.len() <= 320 && !e.chars().any(char::is_control)),
        acknowledged: rt.registration.acknowledged,
    };
    t.check()?;
    rt.remember(r, c)?;
    if let Err(e) = t.check() {
        rt.forget()?;
        return Err(e);
    }
    Ok(())
}
fn revoke(registration: &Registration, credentials: Option<&Credentials>) -> Result<(), String> {
    let Some(c) = credentials else { return Ok(()) };
    let discovery = fetch_json("https://auth.openai.com/.well-known/openid-configuration")?;
    let endpoint = discovery.get("revocation_endpoint").and_then(Value::as_str).ok_or(AUTH_ERROR)?;
    let url = Url::parse(endpoint).map_err(|_| AUTH_ERROR)?;
    if url.scheme() != "https" || url.host_str() != Some("auth.openai.com") {
        return Err(AUTH_ERROR.into());
    }
    form(endpoint, &[("token", &c.refresh_token), ("token_type_hint", "refresh_token"), ("client_id", &registration.client_id)])?;
    Ok(())
}

fn request(rt: &mut Runtime, req: HttpRequest, t: &Ticket) -> Result<String, String> {
    t.check()?;
    if !rt.registration.acknowledged {
        return Err("Review ChatGPT usage controls and choose Got it first.".into());
    }
    let inference = req.method == Method::Post && req.url == format!("{}/responses", responses::BASE);
    if !(inference || (req.method == Method::Get && req.url == format!("{}/models", responses::BASE))) {
        return Err("Unsupported ChatGPT request.".into());
    }
    if inference {
        let body: Value = serde_json::from_str(&req.body).map_err(|_| "Invalid ChatGPT request.")?;
        if body.get("store") != Some(&Value::Bool(false)) || body.get("stream") != Some(&Value::Bool(true)) {
            return Err("ChatGPT requires store: false and stream: true.".into());
        }
    }
    rt.refresh(|r, c| {
        form(
            TOKEN,
            &[("grant_type", "refresh_token"), ("client_id", &r.client_id), ("refresh_token", &c.refresh_token), ("resource", responses::BASE)],
        )
    })?;
    t.check()?;
    let c = rt.credentials.as_ref().ok_or(AUTH_ERROR)?;
    let mut r = match req.method {
        Method::Get => client().get(&req.url).header("authorization", &format!("Bearer {}", c.access_token)).call(),
        Method::Post => client()
            .post(&req.url)
            .header("authorization", &format!("Bearer {}", c.access_token))
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .send(&req.body),
    }
    .map_err(|_| "ChatGPT network request failed or timed out.")?;
    t.check()?;
    if !r.status().is_success() {
        let status = r.status().as_u16();
        if status == 401 {
            rt.forget()?;
            return Err(AUTH_ERROR.into());
        }
        let body = r.body_mut().with_config().limit(MAX_BODY).read_to_string().map_err(|_| "Could not read ChatGPT error response.")?;
        if status == 429 {
            return Err(responses::LIMIT.into());
        }
        return Err(responses::failure(&serde_json::from_str(&body).unwrap_or(Value::Null)));
    }
    if !inference {
        return r.body_mut().with_config().limit(MAX_BODY).read_to_string().map_err(|_| "Could not read ChatGPT models.".into());
    }
    let mut reader = BufReader::new(r.body_mut().as_reader().take(MAX_BODY.saturating_add(1)));
    let mut stream = responses::Stream::default();
    let mut total = 0u64;
    loop {
        t.check()?;
        let mut line = String::new();
        let n = reader.read_line(&mut line).map_err(|_| "ChatGPT stream interrupted; no pending tools were executed.")?;
        total = total.saturating_add(n as u64);
        if total > MAX_BODY {
            return Err("ChatGPT response too large.".into());
        }
        if n == 0 {
            break;
        }
        stream.line(&line)?;
    }
    t.check()?;
    let body = stream.finish()?;
    responses::parse(&body)?;
    Ok(body)
}

fn redact(body: &str, credentials: Option<&Credentials>) -> String {
    let mut body = body.to_string();
    if let Some(c) = credentials {
        for token in [&c.access_token, &c.refresh_token, &c.id_token] {
            if !token.is_empty() {
                body = body.replace(token, "[redacted]");
            }
        }
    }
    body
}

type SharedRuntime = Arc<Mutex<Option<Runtime>>>;
struct Native {
    runtime: SharedRuntime,
    status: Arc<Mutex<Status>>,
    epoch: Arc<AtomicU64>,
    request_epoch: Arc<AtomicU64>,
    http: Box<Http>,
    ctx: egui::Context,
}
impl Native {
    fn task(&mut self, disconnecting: bool, job: impl FnOnce(&mut Runtime, &Ticket) -> Result<(), String> + Send + 'static) {
        if self.status().disconnecting {
            return;
        }
        self.request_epoch.fetch_add(1, Ordering::SeqCst);
        let value = self.epoch.fetch_add(1, Ordering::SeqCst).saturating_add(1);
        let ticket = Ticket { epoch: self.epoch.clone(), value };
        {
            let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner());
            status.busy = true;
            status.disconnecting = disconnecting;
            status.generation = value;
            status.error = None;
        }
        let (runtime, status, ctx) = (self.runtime.clone(), self.status.clone(), self.ctx.clone());
        let err_status = status.clone();
        if std::thread::Builder::new()
            .name("chatgpt-account".into())
            .spawn(move || {
                let mut guard = runtime.lock().unwrap_or_else(|e| e.into_inner());
                let result = (|| {
                    ticket.check()?;
                    if guard.is_none() {
                        *guard = Some(Runtime::load(Box::new(Keychain), Some(lock_host()?))?);
                    }
                    let runtime = guard.as_mut().ok_or(STORAGE_ERROR)?;
                    job(runtime, &ticket)
                })();
                if ticket.check().is_ok() {
                    let mut s = guard.as_ref().map_or(Status::default(), |r| r.status(value));
                    s.error = result.err();
                    *status.lock().unwrap_or_else(|e| e.into_inner()) = s;
                    ctx.request_repaint();
                }
            })
            .is_err()
        {
            let mut s = err_status.lock().unwrap_or_else(|e| e.into_inner());
            s.busy = false;
            s.disconnecting = false;
            s.error = Some("Could not start ChatGPT operation.".into());
        }
    }
    fn disconnect_with(&mut self, revoke: impl FnOnce(&Registration, Option<&Credentials>) -> Result<(), String> + Send + 'static) {
        self.task(true, move |rt, _| {
            let credentials = rt.credentials.clone();
            rt.forget()?;
            if revoke(&rt.registration, credentials.as_ref()).is_err() {
                return Err("Disconnected locally. Remote revocation was not confirmed; disconnect WordCraft in ChatGPT settings.".into());
            }
            Ok(())
        });
    }
}
impl Connection for Native {
    fn status(&self) -> Status {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    fn sign_in(&mut self) {
        self.task(false, sign_in);
    }
    fn cancel(&mut self) {
        // Local removal must finish even if cancellation arrives while the worker is queued.
        if self.status().disconnecting {
            return;
        }
        self.request_epoch.fetch_add(1, Ordering::SeqCst);
        self.epoch.fetch_add(1, Ordering::SeqCst);
        let mut s = self.status.lock().unwrap_or_else(|e| e.into_inner());
        s.busy = false;
        s.generation = self.epoch.load(Ordering::SeqCst);
        s.error = Some(CANCELLED.into());
    }
    fn disconnect(&mut self) {
        self.disconnect_with(revoke);
    }
    fn acknowledge_usage(&mut self) {
        self.task(false, |rt, _| {
            let mut r = rt.registration.clone();
            r.acknowledged = true;
            if let Some(c) = rt.credentials.clone() {
                rt.remember(r, c)?;
            } else {
                return Err("Connect ChatGPT before acknowledging usage.".into());
            }
            Ok(())
        });
    }
    fn cancel_requests(&mut self) {
        self.request_epoch.fetch_add(1, Ordering::SeqCst);
    }
    fn http(&self) -> &Http {
        self.http.as_ref()
    }
}
fn lock_host() -> Result<std::fs::File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    let dir = wordcraft_control_key::settings_dir().ok_or(STORAGE_ERROR)?;
    std::fs::create_dir_all(&dir).map_err(|_| STORAGE_ERROR)?;
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(dir.join("chatgpt-prototype.lock"))
        .map_err(|_| STORAGE_ERROR)?;
    f.try_lock().map_err(|_| "Another WordCraft prototype is using this ChatGPT connection. Close it first.")?;
    Ok(f)
}
pub fn connection(ctx: &egui::Context) -> Box<dyn Connection> {
    let runtime: SharedRuntime = Arc::new(Mutex::new(None));
    let status = Arc::new(Mutex::new(Status::default()));
    let epoch = Arc::new(AtomicU64::new(0));
    let request_epoch = Arc::new(AtomicU64::new(0));
    let (rt, st, ep, context) = (runtime.clone(), status.clone(), request_epoch.clone(), ctx.clone());
    let http: Box<Http> = Box::new(move |req| {
        let (tx, rx) = mpsc::channel();
        let (rt, st, context) = (rt.clone(), st.clone(), context.clone());
        let t = Ticket { epoch: ep.clone(), value: ep.load(Ordering::SeqCst) };
        let inference = req.method == Method::Post;
        let fallback = tx.clone();
        if std::thread::Builder::new()
            .name("chatgpt-responses".into())
            .spawn(move || {
                let mut guard = rt.lock().unwrap_or_else(|e| e.into_inner());
                let result = guard.as_mut().ok_or_else(|| STORAGE_ERROR.to_string()).and_then(|r| {
                    let result = request(r, req, &t);
                    result.map(|body| redact(&body, r.credentials.as_ref()))
                });
                if t.check().is_ok() {
                    if let Some(rt) = guard.as_ref() {
                        let mut s = st.lock().unwrap_or_else(|e| e.into_inner());
                        s.connected = rt.credentials.is_some();
                        s.error = result.as_ref().err().cloned();
                        if !s.connected {
                            s.account = None;
                            s.verified_request = false;
                        }
                        if inference && result.is_ok() {
                            s.verified_request = true;
                        }
                    }
                    let _ = tx.send(result);
                }
                context.request_repaint();
            })
            .is_err()
        {
            let _ = fallback.send(Err("Could not start ChatGPT request.".into()));
        }
        rx
    });
    let mut native = Native { runtime, status, epoch, request_epoch, http, ctx: ctx.clone() };
    let rt = native.runtime.clone();
    // Initialization also runs off the UI thread; no Keychain prompt can block a frame.
    let status = native.status.clone();
    let initial_epoch = native.epoch.clone();
    let ctx = ctx.clone();
    {
        status.lock().unwrap_or_else(|e| e.into_inner()).busy = true;
    }
    if std::thread::Builder::new()
        .name("chatgpt-keychain".into())
        .spawn(move || {
            let mut runtime = rt.lock().unwrap_or_else(|e| e.into_inner());
            let result = lock_host().and_then(|f| Runtime::load(Box::new(Keychain), Some(f)));
            match result {
                Ok(r) => {
                    let s = r.status(0);
                    *runtime = Some(r);
                    if initial_epoch.load(Ordering::SeqCst) == 0 {
                        *status.lock().unwrap_or_else(|e| e.into_inner()) = s;
                    }
                }
                Err(e) => {
                    let mut s = status.lock().unwrap_or_else(|e| e.into_inner());
                    s.busy = false;
                    s.error = Some(e);
                }
            }
            ctx.request_repaint();
        })
        .is_err()
    {
        native.cancel();
    }
    Box::new(native)
}

#[cfg(test)]
#[path = "chatgpt_tests.rs"]
mod tests;
