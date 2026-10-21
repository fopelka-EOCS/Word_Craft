//! Synthetic signing key for mock OAuth endpoints only; never used by the application.
use super::*;
use serde_json::json;
use std::collections::BTreeMap;
const TEST_DER: &str = "MIIEpAIBAAKCAQEAm/Z4KJDKC9hHaoeqWOaKOI5f+VUS71FDxLuEkycQDrU/FIIzHJ3KQiqydmlSC0sG9dknQbJWXdJDIpYK60Fw5wbuWHTueMBinMxoyscunC9+NwADHVyjHXXf3ARTdLXezx9i6JRpw9X36ovvL/Khmxpec1zSGVl7BzXnavstiiCJEazvpaL29/vCT53aiODhzVysTBt20kQNrBowWL1FYUG7+XGt+n8kXo7+xXdzbdzOJ0Hb1o4XBfx81On+WVhKDHi1qnlVZs5x6vSkv/nUyyqtwb/5/kRGVV5G2Nxq/q4n1Ge7pt1iHd+0e4RsR/4djhrr6E8P+mQrkWJEtx/SAwIDAQABAoIBACLF7Tx+K85ARowF+4Eexaqec9+PjE03fTib5o84omflGU9lSCgVNkdSZb/o85X5AHBIHmyTapY4jydi9MRHLvkTnoiyaYkau+Hh4Wt0xXulSyrQIeGmCehDo8nWXRUkh6z+fnsEvCAh0MjGsmmqW0XNt0lZZK6qOLf+2L4NirXE+jAX6XCy2Uv80zMSG5T5jIlWrWS9YvUnNoT00G66ffjBnWHz3PcJMTQRTfAtEvT+hSqPRf7rmjZ6FPygh1HYz6+iWMzE78S1TygZlpTNE+KYoQrGOz3Llj0Nt6+QAEVK6oiNMQdNraPg+zrO6s3rLqNgRvHUEEPonZNIpJB/YkECgYEAyN+8BLkgDQO2UfsKtEKbUEN6XzWFG/Dpcr+E073ApGzNPG1LTm3P7zH+dEC0heZF9acyLKz/SKdACrKK4JyjH8mf6ZiFu5a3ArVe02Zqe7Dsy8gLKbTdCgmZmMtaar4eCQyH+qVv05AC/prQUWNvu60PbOBqlRJbDsylnpKSIasCgYEAxsOHfcywtfnFNS9o6k14WX6db2/1vQRbCAjoPcdLvHBuyMm6VG0cYx/EJ4pilXlEZP2Q1K+7W1VXHtUeFVsnpd+ULowRy645bN87/8VHJ330p/XLafhpejwkL2WaZDuaASWBQl0LW+33E2/6b76J0UKvOyg3atvTi7S4ToYI6QkCgYEAsANsKu9UaFMCZR+Gp6Fdmp0mE1tAQdV9mbC74RX6QZmc1QAn5pdOhpfWEWPJSzMVxkU1L/R+1NjIwxbjniFixq3T08p0FIXh4xut9ateTvge5glpzqfOr8btsfcuSS5B/0fdtwFT1ZvxI8vgEhEBjP/97jdO7UTiSTuFPEfOiHUCgYA/xMeZgqxHFeVBVP6R/VzZSZf1ArSSee1CvKjV+TTBNkk0vhO7tm3fF3f1P561WfypcfifY+n2OYED8Vf/fglQXFTMQkALWtAtr8kuG/riHrT5lhA25W3hu53YXv6TndOg/olj4zobulMEt25fn3/T1VMm5ZYyAGj0b3yaJMKDEQKBgQCXux57UFy0Sg72ZuXWN4lcUoBRtMA77RUDBFoqLBh56sxEH82CacroU5RJplIDw+WN2tsheFF5bj/UqDBkiBGAG60MQHG55/CQyo/pjPZYLyza8yYAAQ4tRxOlT8ZvMHS7kftx0mJMxY202C0n+dFR8eOjKFt0x4spZ+QEID/Z/w==";
const TEST_N: &str = "m_Z4KJDKC9hHaoeqWOaKOI5f-VUS71FDxLuEkycQDrU_FIIzHJ3KQiqydmlSC0sG9dknQbJWXdJDIpYK60Fw5wbuWHTueMBinMxoyscunC9-NwADHVyjHXXf3ARTdLXezx9i6JRpw9X36ovvL_Khmxpec1zSGVl7BzXnavstiiCJEazvpaL29_vCT53aiODhzVysTBt20kQNrBowWL1FYUG7-XGt-n8kXo7-xXdzbdzOJ0Hb1o4XBfx81On-WVhKDHi1qnlVZs5x6vSkv_nUyyqtwb_5_kRGVV5G2Nxq_q4n1Ge7pt1iHd-0e4RsR_4djhrr6E8P-mQrkWJEtx_SAw";

#[derive(Clone, Default)]
struct Memory {
    data: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    fail: Arc<AtomicU64>,
    fail_remove: Arc<AtomicU64>,
}
impl Store for Memory {
    fn load(&self, n: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.data.lock().unwrap().get(n).cloned())
    }
    fn save(&self, n: &str, b: &[u8]) -> Result<(), String> {
        if self.fail.load(Ordering::SeqCst) != 0 {
            return Err(STORAGE_ERROR.into());
        }
        self.data.lock().unwrap().insert(n.into(), b.to_vec());
        Ok(())
    }
    fn remove(&self, n: &str) -> Result<(), String> {
        if self.fail_remove.load(Ordering::SeqCst) != 0 {
            return Err(STORAGE_ERROR.into());
        }
        self.data.lock().unwrap().remove(n);
        Ok(())
    }
}
fn ticket() -> Ticket {
    Ticket { epoch: Arc::new(AtomicU64::new(0)), value: 0 }
}
fn claims(a: &Attempt) -> Value {
    json!({"sub":"synthetic-user","email":"writer@example.test","iss":ISSUER,"aud":"oaiapp_wordcraft_test","exp":now()+3600,"iat":now(),"nonce":a.nonce})
}
fn signed(claims: &Value) -> String {
    let der = base64::engine::general_purpose::STANDARD.decode(TEST_DER).unwrap();
    let mut h = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    h.kid = Some("test-key".into());
    jsonwebtoken::encode(&h, claims, &jsonwebtoken::EncodingKey::from_rsa_der(&der)).unwrap()
}
fn keys() -> Value {
    json!({"keys":[{"kty":"RSA","kid":"test-key","use":"sig","alg":"RS256","n":TEST_N,"e":"AQAB"}]})
}
fn token_response(id_token: &str) -> Value {
    json!({"access_token":"synthetic-access","refresh_token":"synthetic-refresh","id_token":id_token,"token_type":"Bearer","expires_in":3600,"scope":"openid offline_access resource.invoke chatgpt.tokens.use.direct"})
}
fn mock_endpoint(body: Value) -> (String, std::thread::JoinHandle<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    let h = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut reader = BufReader::new(s.try_clone().unwrap());
        let mut raw = String::new();
        let mut len = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            raw.push_str(&line);
            if line == "\r\n" {
                break;
            }
            if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                len = v.trim().parse().unwrap();
            }
        }
        let mut b = vec![0u8; len];
        reader.read_exact(&mut b).unwrap();
        raw.push_str(&String::from_utf8(b).unwrap());
        let body = body.to_string();
        write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        raw
    });
    (url, h)
}
#[test]
fn oauth_success_uses_mock_endpoints_and_remembers_secure_login() {
    let store = Memory::default();
    let mut rt = Runtime::load(Box::new(store.clone()), None).unwrap();
    let a = Attempt::new(1455, &rt.registration).unwrap();
    let url = a.url(&rt.registration, None).unwrap();
    let q: BTreeMap<_, _> = url.query_pairs().collect();
    assert_eq!(q["client_id"], "dynamic_agent_client");
    assert_eq!(q["agent_name_hint"], "WordCraft");
    assert_eq!(q.get("resource").map(|value| value.as_ref()), Some(responses::BASE));
    assert_eq!(q["code_challenge"], URL_SAFE_NO_PAD.encode(Sha256::digest(a.verifier.as_bytes())));
    let target = format!("/auth/callback?state={}&code=one-time-code&client_id=oaiapp_wordcraft_test", a.state);
    let (code, id) = a.callback(&target).unwrap();
    let (url, seen) = mock_endpoint(token_response(&signed(&claims(&a))));
    let (jwks, keys_seen) = mock_endpoint(keys());
    complete_sign_in(
        &mut rt,
        &a,
        &id,
        &ticket(),
        || form(&url, &[("client_id", &id), ("code", &code), ("code_verifier", &a.verifier)]),
        || fetch_json(&jwks),
    )
    .unwrap();
    assert!(seen.join().unwrap().contains("code=one-time-code"));
    keys_seen.join().unwrap();
    let state = serde_json::to_string(&rt.status(0)).unwrap();
    assert!(state.contains("writer@example.test"));
    assert!(!state.contains("synthetic-access"));
    assert!(!state.contains("synthetic-refresh"));
    let restarted = Runtime::load(Box::new(store), None).unwrap();
    assert!(restarted.credentials.is_some());
    assert_eq!(restarted.registration.host, rt.registration.host);
    assert_eq!(restarted.registration.client_id, id);
    let next = Attempt::new(54321, &restarted.registration).unwrap();
    let u = next.url(&restarted.registration, restarted.credentials.as_ref()).unwrap();
    let q: BTreeMap<_, _> = u.query_pairs().collect();
    assert_eq!(q["client_id"], id);
    assert_eq!(q.get("resource").map(|value| value.as_ref()), Some(responses::BASE));
    assert!(!q.contains_key("agent_name_hint"));
    assert!(q.contains_key("id_token_hint"));
}
#[test]
fn invalid_callback_state_duplicate_fields_or_changed_client_are_rejected() {
    let rt = Runtime::load(Box::new(Memory::default()), None).unwrap();
    let mut a = Attempt::new(1455, &rt.registration).unwrap();
    for target in ["/auth/callback?code=code", "/auth/callback?state=wrong&code=code", "/callback?code=code"] {
        assert!(a.callback(target).is_err());
    }
    assert!(a.callback(&format!("/auth/callback?state={}&state={}&code=c&client_id=oaiapp_a", a.state, a.state)).is_err());
    assert!(a.callback(&format!("/auth/callback?state={}&error=access_denied", a.state)).unwrap_err().contains("declined"));
    assert!(a.callback(&format!("/auth/callback?state={}&code=c&client_id=dynamic_agent_client", a.state)).is_err());
    a.client_id = "oaiapp_original".into();
    assert!(a.callback(&format!("/auth/callback?state={}&code=c&client_id=oaiapp_other", a.state)).is_err());
}
#[test]
fn callback_cancellation_timeout_and_loopback_success() {
    let rt = Runtime::load(Box::new(Memory::default()), None).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let a = Attempt::new(port, &rt.registration).unwrap();
    let t = ticket();
    t.epoch.store(1, Ordering::SeqCst);
    assert_eq!(callback(&listener, &a, &t, Duration::from_secs(1)).unwrap_err(), CANCELLED);
    assert!(callback(&listener, &a, &ticket(), Duration::ZERO).unwrap_err().contains("timed out"));
    let url = format!("{}?state={}&code=c&client_id=oaiapp_test", a.redirect, a.state);
    let h = std::thread::spawn(move || client().get(&url).call().unwrap().body_mut().read_to_string().unwrap());
    assert_eq!(callback(&listener, &a, &ticket(), Duration::from_secs(3)).unwrap(), ("c".into(), "oaiapp_test".into()));
    assert!(!h.join().unwrap().contains("state="));
}
#[test]
fn validates_signature_issuer_audience_expiry_nonce_and_subject() {
    let rt = Runtime::load(Box::new(Memory::default()), None).unwrap();
    let a = Attempt::new(1455, &rt.registration).unwrap();
    let good = claims(&a);
    assert!(identity(&signed(&good), "oaiapp_wordcraft_test", &a.nonce, &keys()).is_ok());
    for (field, bad) in
        [("iss", json!("https://evil.test")), ("aud", json!("wrong")), ("exp", json!(now() - 20)), ("nonce", json!("wrong")), ("sub", json!(""))]
    {
        let mut v = good.clone();
        v[field] = bad;
        assert!(identity(&signed(&v), "oaiapp_wordcraft_test", &a.nonce, &keys()).is_err(), "{field}");
    }
    let mut token = signed(&good);
    token.push('x');
    assert!(identity(&token, "oaiapp_wordcraft_test", &a.nonce, &keys()).is_err());
}
#[test]
fn refresh_rotation_failure_credential_removal_and_storage_failure() {
    let store = Memory::default();
    let mut rt = Runtime::load(Box::new(store.clone()), None).unwrap();
    let mut r = rt.registration.clone();
    r.client_id = "oaiapp_test".into();
    r.subject = "subject".into();
    r.acknowledged = true;
    let mut c = tokens(&token_response("id-token"), None).unwrap();
    c.expires_at = 0;
    rt.remember(r, c).unwrap();
    let mut v = token_response("new-id");
    v["refresh_token"] = json!("rotated");
    rt.refresh(|r, _| {
        assert_eq!(r.client_id, "oaiapp_test");
        Ok(v)
    })
    .unwrap();
    assert_eq!(rt.credentials.as_ref().unwrap().refresh_token, "rotated");
    let restarted = Runtime::load(Box::new(store.clone()), None).unwrap();
    assert_eq!(restarted.credentials.unwrap().refresh_token, "rotated");
    rt.credentials.as_mut().unwrap().expires_at = 0;
    assert!(rt.refresh(|_, _| Err(AUTH_ERROR.into())).is_err());
    assert!(rt.credentials.is_none());
    assert!(store.load("credentials").unwrap().is_none());
    rt.forget().unwrap();
    assert!(store.load("registration").unwrap().is_some());
    let c = tokens(&token_response("id-token"), None).unwrap();
    store.fail.store(1, Ordering::SeqCst);
    let e = rt.remember(rt.registration.clone(), c).unwrap_err();
    assert_eq!(e, STORAGE_ERROR);
    assert!(rt.credentials.is_none());
}
#[test]
fn cancelled_attempt_or_missing_usage_permission_never_saves_tokens() {
    let store = Memory::default();
    let mut rt = Runtime::load(Box::new(store.clone()), None).unwrap();
    let a = Attempt::new(1455, &rt.registration).unwrap();
    let t = ticket();
    t.epoch.store(1, Ordering::SeqCst);
    assert!(complete_sign_in(&mut rt, &a, "oaiapp_test", &t, || Err("must not exchange".into()), || Ok(keys())).is_err());
    let mut v = token_response("synthetic-id");
    v["scope"] = json!("openid");
    assert!(tokens(&v, None).is_err());
    assert!(store.load("credentials").unwrap().is_none());
    let req = HttpRequest { method: Method::Post, url: format!("{}/responses", responses::BASE), headers: vec![], body: "private document".into() };
    assert!(request(&mut rt, req, &ticket()).unwrap_err().contains("Got it"));
}

#[test]
fn all_retained_tokens_are_redacted_before_ui_or_control_delivery() {
    let c = tokens(&token_response("synthetic-id"), None).unwrap();
    let body=json!({"status":"completed","output":[{"type":"message","status":"completed","content":[{"type":"output_text","text":"synthetic-access synthetic-refresh synthetic-id"}]}]}).to_string();
    let clean = redact(&body, Some(&c));
    let reply = responses::parse(&clean).unwrap();
    assert_eq!(reply.text, "[redacted] [redacted] [redacted]");
}

#[test]
fn cancelling_queued_disconnect_cannot_keep_remembered_credentials() {
    let store = Memory::default();
    let mut rt = Runtime::load(Box::new(store.clone()), None).unwrap();
    rt.remember(rt.registration.clone(), tokens(&token_response("id"), None).unwrap()).unwrap();
    let runtime = Arc::new(Mutex::new(Some(rt)));
    let guard = runtime.lock().unwrap();
    let mut native = Native {
        runtime: runtime.clone(),
        status: Arc::new(Mutex::new(Status { connected: true, ..Default::default() })),
        epoch: Arc::new(AtomicU64::new(0)),
        request_epoch: Arc::new(AtomicU64::new(0)),
        http: Box::new(|_| mpsc::channel().1),
        ctx: egui::Context::default(),
    };
    native.disconnect_with(|_, _| Ok(()));
    native.cancel();
    native.sign_in();
    native.acknowledge_usage();
    drop(guard);
    let deadline = Instant::now() + Duration::from_secs(3);
    while store.load("credentials").unwrap().is_some() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(store.load("credentials").unwrap().is_none(), "Disconnect must remove credentials even when cancelled while queued");
    assert!(Runtime::load(Box::new(store), None).unwrap().credentials.is_none());
}

#[test]
fn slow_callback_line_obeys_deadline_and_cancellation() {
    for cancel in [false, true] {
        let rt = Runtime::load(Box::new(Memory::default()), None).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let a = Attempt::new(port, &rt.registration).unwrap();
        let t = ticket();
        let epoch = t.epoch.clone();
        let (started, ready) = mpsc::channel();
        let writer = std::thread::spawn(move || {
            let mut socket = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            socket.write_all(b"G").unwrap();
            started.send(()).unwrap();
            for _ in 0..100 {
                std::thread::sleep(Duration::from_millis(10));
                if socket.write_all(b"x").is_err() {
                    break;
                }
            }
        });
        ready.recv().unwrap();
        let (done, result) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let outcome = callback(&listener, &a, &t, if cancel { Duration::from_secs(5) } else { Duration::from_millis(50) });
            done.send(outcome).unwrap();
        });
        if cancel {
            std::thread::sleep(Duration::from_millis(50));
            epoch.store(1, Ordering::SeqCst);
        }
        let outcome = result.recv_timeout(Duration::from_millis(500));
        writer.join().unwrap();
        reader.join().unwrap();
        let error = outcome.expect("a slow request line must not delay timeout or cancellation").unwrap_err();
        assert!(if cancel { error == CANCELLED } else { error.contains("timed out") });
    }
}

#[test]
fn fragmented_callback_request_line_is_accepted() {
    let rt = Runtime::load(Box::new(Memory::default()), None).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let a = Attempt::new(port, &rt.registration).unwrap();
    let tail = format!("callback?state={}&code=c&client_id=oaiapp_test HTTP/1.1\r\n\r\n", a.state);
    let writer = std::thread::spawn(move || {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(b"GET /auth/").unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let _ = s.write_all(tail.as_bytes());
        let mut response = String::new();
        let _ = s.read_to_string(&mut response);
    });
    let result = callback(&listener, &a, &ticket(), Duration::from_secs(2));
    writer.join().unwrap();
    assert_eq!(result.unwrap(), ("c".into(), "oaiapp_test".into()));
}

#[test]
fn disconnect_deletion_failure_can_be_retried_and_remote_failure_is_explicit() {
    let store = Memory::default();
    let mut rt = Runtime::load(Box::new(store.clone()), None).unwrap();
    rt.remember(rt.registration.clone(), tokens(&token_response("id"), None).unwrap()).unwrap();
    let mut native = Native {
        runtime: Arc::new(Mutex::new(Some(rt))),
        status: Arc::new(Mutex::new(Status { connected: true, ..Default::default() })),
        epoch: Arc::new(AtomicU64::new(0)),
        request_epoch: Arc::new(AtomicU64::new(0)),
        http: Box::new(|_| mpsc::channel().1),
        ctx: egui::Context::default(),
    };
    for (metadata_failure, deletion_failure) in [(true, false), (false, true), (false, false)] {
        let fail = metadata_failure || deletion_failure;
        store.fail.store(u64::from(metadata_failure), Ordering::SeqCst);
        store.fail_remove.store(u64::from(deletion_failure), Ordering::SeqCst);
        native.disconnect_with(|_, _| Err("synthetic revocation failure".into()));
        let deadline = Instant::now() + Duration::from_secs(3);
        while native.status().busy && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let status = native.status();
        assert!(!status.busy);
        assert!(!status.disconnecting);
        assert_eq!(status.connected, fail);
        assert_eq!(Runtime::load(Box::new(store.clone()), None).unwrap().credentials.is_some(), fail);
        let error = status.error.unwrap();
        assert!(error.contains(if fail { "Disconnect again" } else { "Remote revocation was not confirmed" }));
        assert!(!error.contains("synthetic"));
    }
}

#[test]
fn failed_refresh_and_failed_deletion_do_not_claim_persistent_sign_out() {
    let store = Memory::default();
    let mut rt = Runtime::load(Box::new(store.clone()), None).unwrap();
    let mut credentials = tokens(&token_response("id"), None).unwrap();
    credentials.expires_at = 0;
    rt.remember(rt.registration.clone(), credentials).unwrap();
    store.fail_remove.store(1, Ordering::SeqCst);
    assert!(rt.refresh(|_, _| Err(AUTH_ERROR.into())).is_err());
    assert!(rt.credentials.is_some());
    assert!(Runtime::load(Box::new(store.clone()), None).unwrap().credentials.is_some());
    store.fail_remove.store(0, Ordering::SeqCst);
    rt.forget().unwrap();
    assert!(Runtime::load(Box::new(store), None).unwrap().credentials.is_none());
}

#[test]
fn failed_credential_update_preserves_the_previous_atomic_state() {
    let store = Memory::default();
    let mut rt = Runtime::load(Box::new(store.clone()), None).unwrap();
    rt.remember(rt.registration.clone(), tokens(&token_response("id"), None).unwrap()).unwrap();
    let previous = store.load("credentials").unwrap();
    let mut registration = rt.registration.clone();
    registration.acknowledged = true;
    store.fail.store(1, Ordering::SeqCst);
    assert!(rt.remember(registration, tokens(&token_response("changed-id"), None).unwrap()).is_err());
    assert!(rt.credentials.is_some());
    assert!(!rt.registration.acknowledged);
    assert_eq!(rt.credentials.as_ref().unwrap().id_token, "id");
    assert_eq!(store.load("credentials").unwrap(), previous);
}

#[test]
fn runtime_keeps_the_exclusive_lock_for_the_credential_lifetime() {
    let path = std::env::temp_dir().join(format!("wordcraft-oauth-lock-{}", random().unwrap()));
    let first = std::fs::OpenOptions::new().create_new(true).read(true).write(true).open(&path).unwrap();
    first.try_lock().unwrap();
    let rt = Runtime::load(Box::new(Memory::default()), Some(first)).unwrap();
    let second = std::fs::OpenOptions::new().read(true).write(true).open(&path).unwrap();
    assert!(second.try_lock().is_err());
    drop(rt);
    second.try_lock().unwrap();
    drop(second);
    std::fs::remove_file(path).unwrap();
}
