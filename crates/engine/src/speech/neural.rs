//! Neural voices: a local voice program (Piper, or any program the user sets) or an
//! OpenAI-compatible cloud service makes a WAV file for each sentence, and the system's audio
//! player plays it. The voices each engine has are listed here too, for the settings' lists.
//!
//! The cloud request goes through the system's `curl` (macOS, Windows 10 and later, Linux), so
//! WordCraft needs no HTTP or TLS library. The API key goes to `curl` on stdin, never on the
//! command line, and the sentence goes in a request file, never in an argument. The functions
//! that build the commands are plain, so the tests can check them on every system.

use std::path::{Path, PathBuf};

use serde_json::json;

use super::settings::{CloudVoice, LocalVoice, endpoint_ok, env_name_ok};

/// Longest wait for one sentence from the cloud service, in seconds.
const CLOUD_TIMEOUT_SECS: &str = "60";

/// One program to run: the program, its arguments, what goes on its stdin and extra environment
/// variables.
#[derive(Debug, PartialEq)]
pub(crate) struct Run {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub stdin: Vec<u8>,
    pub env: Vec<(String, String)>,
}

/// Piper's `--length_scale` for a speed: the length of the speech, the inverse of the speed.
pub(crate) fn length_scale(rate: f32) -> f32 {
    let r = if rate.is_finite() { rate.clamp(0.25, 4.0) } else { 1.0 };
    1.0 / r
}

/// The local voice program for one sentence: `voice` (the role's, else the default), the speed
/// and the WAV file to write fill the argument template; the sentence goes on stdin.
pub(crate) fn local_run(local: &LocalVoice, voice: &str, rate: f32, out: &Path, text: &str) -> Result<Run, String> {
    let voice = if voice.is_empty() { local.voice.as_str() } else { voice };
    if voice.is_empty() && local.args.iter().any(|a| a.contains("{voice}")) {
        return Err("Read Aloud needs a voice model for the local voice: set it in File › Options › Accessibility".into());
    }
    let out = out.to_string_lossy();
    let rate_s = format!("{:.2}", if rate.is_finite() { rate } else { 1.0 });
    let scale = format!("{:.3}", length_scale(rate));
    let args = local
        .args
        .iter()
        .map(|a| a.replace("{voice}", voice).replace("{output}", &out).replace("{rate}", &rate_s).replace("{lengthScale}", &scale))
        .collect();
    let mut stdin = text.as_bytes().to_vec();
    stdin.push(b'\n');
    Ok(Run { program: PathBuf::from(&local.program), args, stdin, env: Vec::new() })
}

/// Is `key` safe in a `curl` config line: printable ASCII without quotes or backslashes.
fn key_ok(key: &str) -> bool {
    !key.is_empty() && key.len() <= 512 && key.bytes().all(|b| (0x21..=0x7E).contains(&b) && b != b'"' && b != b'\\')
}

/// The request body for one sentence.
pub(crate) fn cloud_body(cloud: &CloudVoice, voice: &str, rate: f32, text: &str) -> Vec<u8> {
    let voice = if voice.is_empty() { cloud.voice.as_str() } else { voice };
    let speed = if rate.is_finite() { rate.clamp(0.25, 4.0) } else { 1.0 };
    json!({"model": cloud.model, "voice": voice, "input": text, "response_format": "wav", "speed": speed}).to_string().into_bytes()
}

/// `curl` posting the request file `body` to the cloud service, writing the WAV file `out`.
/// `key` is the API key from the environment variable the settings name.
pub(crate) fn cloud_run(cloud: &CloudVoice, key: Option<&str>, body: &Path, out: &Path) -> Result<Run, String> {
    if !endpoint_ok(&cloud.endpoint) {
        return Err("the cloud voice address must start with https://".into());
    }
    if !env_name_ok(&cloud.api_key_env) {
        return Err("the cloud voice needs the name of the environment variable that holds the API key".into());
    }
    let key = key
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .ok_or_else(|| format!("set the API key for the cloud voice in the {} environment variable", cloud.api_key_env))?;
    if !key_ok(key) {
        return Err(format!("the API key in {} has characters an API key can't have", cloud.api_key_env));
    }
    let args = [
        "-sS",
        "--fail",
        "--max-time",
        CLOUD_TIMEOUT_SECS,
        "-X",
        "POST",
        "--url",
        cloud.endpoint.as_str(),
        "-K",
        "-",
        "-H",
        "Content-Type: application/json",
        "--data-binary",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([format!("@{}", body.to_string_lossy()), "-o".into(), out.to_string_lossy().into_owned()])
    .collect();
    // The key goes in as a `curl` config line on stdin, so other programs can't see it in the
    // list of running processes.
    let stdin = format!("header = \"Authorization: Bearer {key}\"\n").into_bytes();
    Ok(Run { program: PathBuf::from("curl"), args, stdin, env: Vec::new() })
}

/// The programs that can play the WAV file `wav`, best first.
pub(crate) fn players(wav: &Path) -> Vec<Run> {
    let path = wav.to_string_lossy().into_owned();
    let run = |p: &str, args: &[&str]| Run {
        program: PathBuf::from(p),
        args: args.iter().map(|s| s.to_string()).chain([path.clone()]).collect(),
        stdin: Vec::new(),
        env: Vec::new(),
    };
    if cfg!(target_os = "macos") {
        vec![run("afplay", &[])]
    } else if cfg!(target_os = "windows") {
        // The path is in the environment, never in the script.
        vec![Run {
            program: super::windows_powershell(std::env::var_os("SystemRoot")),
            args: ["-NoProfile", "-NonInteractive", "-Command", "(New-Object System.Media.SoundPlayer $env:WORDCRAFT_WAV).PlaySync()"]
                .map(String::from)
                .to_vec(),
            stdin: Vec::new(),
            env: vec![("WORDCRAFT_WAV".into(), path)],
        }]
    } else {
        vec![run("pw-play", &[]), run("paplay", &[]), run("aplay", &["-q"]), run("ffplay", &["-nodisp", "-autoexit", "-loglevel", "quiet"])]
    }
}

/// Read Aloud's private temporary folder: made once per process in the system's temporary
/// folder, with a random name and, on Unix, readable by this user only (mode 0700), so other
/// users can't read the sentences or put a link where a file will be written.
#[cfg(not(target_arch = "wasm32"))]
fn private_dir() -> Result<&'static Path, String> {
    static DIR: std::sync::OnceLock<Result<PathBuf, String>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let mut last = String::from("no temporary folder");
        for _ in 0..8 {
            let mut bytes = [0u8; 8];
            getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
            let name: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            let dir = std::env::temp_dir().join(format!("wordcraft-speech-{}-{name}", std::process::id()));
            let mut b = std::fs::DirBuilder::new();
            #[cfg(unix)]
            std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
            // `create` fails when the folder exists: a folder someone else made is never used.
            match b.create(&dir) {
                Ok(()) => return Ok(dir),
                Err(e) => last = e.to_string(),
            }
        }
        Err(format!("Read Aloud couldn't make its temporary folder: {last}"))
    })
    .as_ref()
    .map(PathBuf::as_path)
    .map_err(Clone::clone)
}

/// A new file name in Read Aloud's private temporary folder, for audio or requests.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn temp_file(ext: &str) -> Result<PathBuf, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    Ok(private_dir()?.join(format!("{n}.{ext}")))
}

/// How a program run ended.
#[derive(Debug, PartialEq)]
pub(crate) enum Ran {
    Exited(bool),
    Interrupted,
    NotStarted(String),
}

/// Run `run` to its end, or stop it when `changed()`.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn run(run: &Run, changed: &dyn Fn() -> bool) -> Ran {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut c = Command::new(&run.program);
    c.args(&run.args).envs(run.env.iter().map(|(k, v)| (k, v)));
    #[cfg(target_os = "windows")]
    std::os::windows::process::CommandExt::creation_flags(&mut c, super::CREATE_NO_WINDOW);
    let mut child = match c.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        Ok(ch) => ch,
        Err(e) => return Ran::NotStarted(e.to_string()),
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&run.stdin);
    }
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ran::Exited(status.success()),
            Ok(None) => {}
            Err(e) => return Ran::NotStarted(e.to_string()),
        }
        if changed() {
            let _ = child.kill();
            let _ = child.wait();
            return Ran::Interrupted;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}

/// Play the WAV file `wav` with the first player that plays it: a player that doesn't start, or
/// fails (`pw-play` without a running PipeWire), hands it to the next one. `Exited(true)` when
/// one played it to the end.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn play(wav: &Path, changed: &dyn Fn() -> bool) -> Ran {
    let mut last = Ran::NotStarted("no audio player on this system (install PipeWire, PulseAudio, ALSA utilities or FFmpeg)".into());
    for p in players(wav) {
        match run(&p, changed) {
            Ran::NotStarted(e) => last = Ran::NotStarted(format!("{}: {e}", p.program.display())),
            Ran::Exited(false) => last = Ran::NotStarted(format!("the audio player {} failed", p.program.display())),
            done => return done,
        }
    }
    last
}

/// Delete a temporary file of Read Aloud's own (never a user's file: the names come from
/// [`temp_file`]).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn remove(path: impl AsRef<Path>) {
    let _ = std::fs::remove_file(path);
}

/// Write the request file of a cloud sentence (a [`temp_file`]): a new file only (never one that
/// is there already, or a link), readable by this user only (mode 0600 on Unix).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn write_request(path: &Path, body: Vec<u8>) -> std::io::Result<()> {
    use std::io::Write;
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(path)?.write_all(&body)
}

/// Does `path` hold some audio?
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn has_audio(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.len() > 44)
}

/// A voice the user can pick: what the settings keep (`id`: a voice name, a language code or a
/// model path) and what the list shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceChoice {
    pub id: String,
    pub label: String,
}

impl VoiceChoice {
    fn new(id: impl Into<String>, label: impl Into<String>) -> VoiceChoice {
        VoiceChoice { id: id.into(), label: label.into() }
    }
    pub fn to_json(&self) -> serde_json::Value {
        json!({"id": self.id, "label": self.label})
    }
}

/// The voices an OpenAI-compatible speech service has (other services may have more: the field
/// takes any name).
pub const CLOUD_VOICES: &[&str] = &["alloy", "ash", "ballad", "coral", "echo", "fable", "nova", "onyx", "sage", "shimmer", "verse"];
/// Its speech models.
pub const CLOUD_MODELS: &[&str] = &["tts-1", "tts-1-hd", "gpt-4o-mini-tts"];
/// Most voices a list holds.
const MAX_CHOICES: usize = 1000;

/// The voices in `espeak-ng --voices`'s `listing`: a header line, then `Pty Language Age/Gender
/// VoiceName File …` per voice. The language code is what `-v` takes.
pub(crate) fn espeak_voices(listing: &str) -> Vec<VoiceChoice> {
    listing
        .lines()
        .skip(1)
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            let (_, lang, _, name) = (w.next()?, w.next()?, w.next()?, w.next()?);
            (!lang.starts_with('-') && lang != "variant").then(|| VoiceChoice::new(lang, format!("{} ({lang})", name.replace('_', " "))))
        })
        .take(MAX_CHOICES)
        .collect()
}

/// The voices in `say -v '?'`'s `listing`, without the novelty voices.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn mac_choices(listing: &str) -> Vec<VoiceChoice> {
    super::mac_voices(listing)
        .filter(|(name, _)| !super::MAC_NOVELTY_VOICES.contains(&name.as_str()) && !name.starts_with('-'))
        .map(|(name, code)| VoiceChoice::new(name.clone(), format!("{name} ({code})")))
        .take(MAX_CHOICES)
        .collect()
}

/// The voices in the Windows listing: `Name|culture` per line.
pub(crate) fn windows_choices(listing: &str) -> Vec<VoiceChoice> {
    listing
        .lines()
        .filter_map(|l| {
            let (name, culture) = l.trim().split_once('|')?;
            let name = name.trim();
            (!name.is_empty() && !name.starts_with('-')).then(|| VoiceChoice::new(name, format!("{name} ({})", culture.trim())))
        })
        .take(MAX_CHOICES)
        .collect()
}

/// The voices of the system's speech program (asked once; `refresh` asks again).
pub fn system_voices(refresh: bool) -> Vec<VoiceChoice> {
    static CACHE: std::sync::Mutex<Option<Vec<VoiceChoice>>> = std::sync::Mutex::new(None);
    let mut c = CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if refresh || c.is_none() {
        *c = Some(ask_system_voices());
    }
    c.clone().unwrap_or_default()
}

/// Ask the speech program for its voices (none in tests, which never start speech programs, and
/// in the browser).
fn ask_system_voices() -> Vec<VoiceChoice> {
    #[cfg(any(test, target_arch = "wasm32"))]
    {
        Vec::new()
    }
    #[cfg(not(any(test, target_arch = "wasm32")))]
    {
        let out = |program: PathBuf, args: &[&str]| {
            let mut c = std::process::Command::new(program);
            c.args(args).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
            #[cfg(target_os = "windows")]
            std::os::windows::process::CommandExt::creation_flags(&mut c, super::CREATE_NO_WINDOW);
            c.output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        };
        if cfg!(target_os = "macos") {
            #[cfg(target_os = "macos")]
            return mac_choices(super::mac_voice_listing());
        }
        if cfg!(target_os = "windows") {
            let script = "Add-Type -AssemblyName System.Speech; (New-Object System.Speech.Synthesis.SpeechSynthesizer).GetInstalledVoices() | Where-Object { $_.Enabled } | ForEach-Object { $_.VoiceInfo.Name + '|' + $_.VoiceInfo.Culture.Name }";
            let ps = super::windows_powershell(std::env::var_os("SystemRoot"));
            return out(ps, &["-NoProfile", "-NonInteractive", "-Command", script]).map(|l| windows_choices(&l)).unwrap_or_default();
        }
        ["espeak-ng", "espeak"].iter().find_map(|p| out(PathBuf::from(p), &["--voices"])).map(|l| espeak_voices(&l)).unwrap_or_default()
    }
}

/// The folders Piper voice models are looked for in: the folders of `known` models (the ones the
/// settings already use), then the usual places voice downloads go.
pub fn piper_dirs(known: &[&str]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = known.iter().filter(|p| !p.is_empty()).filter_map(|p| Path::new(p).parent().map(Path::to_path_buf)).collect();
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| home.as_ref().map(|h| h.join(".local").join("share")));
    for base in [data, std::env::var_os("APPDATA").map(PathBuf::from), std::env::var_os("LOCALAPPDATA").map(PathBuf::from)].into_iter().flatten() {
        dirs.extend(["piper-voices", "piper/voices", "piper"].iter().map(|d| base.join(d)));
    }
    if let Some(h) = &home {
        dirs.extend([
            h.join("piper-voices"),
            h.join(".piper"),
            h.join("Library/Application Support/piper"),
            h.join("Library/Application Support/piper-voices"),
        ]);
    }
    let mut seen = Vec::new();
    dirs.retain(|d| {
        let new = !seen.contains(d);
        seen.push(d.clone());
        new
    });
    dirs
}

/// The Piper voice models (`.onnx` files) in `dirs` and their subfolders one level down, by name.
pub fn piper_models(dirs: &[PathBuf]) -> Vec<VoiceChoice> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = dirs;
        Vec::new()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut out: Vec<VoiceChoice> = Vec::new();
        let mut add = |path: &Path| {
            let is_model = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("onnx"));
            let id = path.to_string_lossy().into_owned();
            if is_model && out.len() < MAX_CHOICES && !id.starts_with('-') && !out.iter().any(|c| c.id == id) {
                let name = path.file_stem().map_or_else(|| id.clone(), |s| s.to_string_lossy().into_owned());
                out.push(VoiceChoice::new(id, name));
            }
        };
        let entries = |d: &Path| std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path()).take(MAX_CHOICES).collect::<Vec<_>>();
        for d in dirs {
            for p in entries(d) {
                if p.is_dir() {
                    entries(&p).iter().for_each(|q| add(q));
                } else {
                    add(&p);
                }
            }
        }
        out.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piper_gets_the_model_speed_and_output() {
        let mut l = LocalVoice::default();
        let out = Path::new("/tmp/x.wav");
        assert!(local_run(&l, "", 1.0, out, "Hi").is_err(), "no model");
        l.voice = "/voices/en_US-amy.onnx".into();
        let r = local_run(&l, "", 2.0, out, "Hi").unwrap();
        assert_eq!(r.program, PathBuf::from("piper"));
        assert_eq!(r.args, ["--model", "/voices/en_US-amy.onnx", "--output_file", "/tmp/x.wav", "--length_scale", "0.500"]);
        assert_eq!(r.stdin, b"Hi\n");
        // The role's voice wins.
        assert_eq!(local_run(&l, "/voices/b.onnx", 1.0, out, "Hi").unwrap().args[1], "/voices/b.onnx");
        // A custom template without a voice needs no model.
        l.voice.clear();
        l.args = vec!["--out".into(), "{output}".into(), "--speed".into(), "{rate}".into()];
        assert_eq!(local_run(&l, "", 1.5, out, "x").unwrap().args, ["--out", "/tmp/x.wav", "--speed", "1.50"]);
    }

    #[test]
    fn cloud_requests_are_private_new_files() {
        let p = temp_file("json").unwrap();
        write_request(&p, b"{}".to_vec()).unwrap();
        // Never over a file (or a link) that is there already.
        assert!(write_request(&p, b"{}".to_vec()).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        }
        remove(&p);
    }

    #[test]
    fn cloud_request_keeps_the_key_and_text_off_the_command_line() {
        let c = CloudVoice::default();
        let body = Path::new("/tmp/b.json");
        let out = Path::new("/tmp/o.wav");
        assert!(cloud_run(&c, None, body, out).is_err());
        assert!(cloud_run(&c, Some("  "), body, out).is_err());
        assert!(cloud_run(&c, Some("sk-a\"b"), body, out).is_err());
        let r = cloud_run(&c, Some("sk-test123"), body, out).unwrap();
        assert_eq!(r.program, PathBuf::from("curl"));
        assert!(r.args.iter().all(|a| !a.contains("sk-test123")));
        assert!(r.args.windows(2).any(|w| w[0] == "--url" && w[1] == c.endpoint));
        assert!(r.args.iter().any(|a| a == "@/tmp/b.json"));
        assert_eq!(r.stdin, b"header = \"Authorization: Bearer sk-test123\"\n");
        let b: serde_json::Value = serde_json::from_slice(&cloud_body(&c, "", 9.0, "Hello \"there\"")).unwrap();
        assert_eq!(b["input"], "Hello \"there\"");
        assert_eq!(b["voice"], "alloy");
        assert_eq!(b["speed"], 4.0);
        assert_eq!(b["response_format"], "wav");
        let mut bad = c.clone();
        bad.endpoint = "http://example.com".into();
        assert!(cloud_run(&bad, Some("k"), body, out).is_err());
    }

    #[test]
    fn system_voice_listings_are_read() {
        let espeak = "Pty Language       Age/Gender VoiceName          File                 Other Languages\n 5  af              --/M      Afrikaans          gmw/af\n 5  en-gb           --/M      English_(Great_Britain) gmw/en  (en 2)\n\n garbage\n";
        assert_eq!(espeak_voices(espeak), [VoiceChoice::new("af", "Afrikaans (af)"), VoiceChoice::new("en-gb", "English (Great Britain) (en-gb)")]);
        let mac = "Albert              en_US    # Hello!\nSamantha            en_US    # Hello!\nEddy (Russian (Russia)) ru_RU    # Привет!\n";
        assert_eq!(
            mac_choices(mac),
            [VoiceChoice::new("Samantha", "Samantha (en_us)"), VoiceChoice::new("Eddy (Russian (Russia))", "Eddy (Russian (Russia)) (ru_ru)")]
        );
        assert_eq!(
            windows_choices("Microsoft Zira Desktop|en-US\n|x\n-bad|en\nnoculture\n"),
            [VoiceChoice::new("Microsoft Zira Desktop", "Microsoft Zira Desktop (en-US)")]
        );
        assert!(system_voices(true).is_empty(), "tests never start a speech program");
    }

    #[test]
    fn piper_models_are_found_in_their_folders() {
        let dir = std::env::temp_dir().join(format!("wordcraft-piper-test-{}", std::process::id()));
        let sub = dir.join("en").join("en_US");
        std::fs::create_dir_all(dir.join("en")).unwrap();
        std::fs::create_dir_all(&sub).unwrap();
        for f in [
            dir.join("b-voice.onnx"),
            dir.join("b-voice.onnx.json"),
            dir.join("en").join("A-voice.ONNX"),
            sub.join("too-deep.onnx"),
            dir.join("notes.txt"),
        ] {
            std::fs::write(f, b"x").unwrap();
        }
        let found = piper_models(&[dir.clone(), dir.clone(), dir.join("missing")]);
        let names: Vec<&str> = found.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(names, ["A-voice", "b-voice"]);
        assert!(found[1].id.ends_with("b-voice.onnx"));
        let _ = std::fs::remove_dir_all(&dir);
        // The folder of a model in use comes first, and no folder twice.
        let dirs = piper_dirs(&["/voices/x.onnx", "", "/voices/y.onnx"]);
        assert_eq!(dirs.first(), Some(&PathBuf::from("/voices")));
        assert_eq!(dirs.iter().filter(|d| d.as_path() == Path::new("/voices")).count(), 1);
    }

    #[test]
    fn audio_players_get_the_file() {
        let p = players(Path::new("/tmp/a.wav"));
        assert!(!p.is_empty());
        for r in &p {
            assert!(r.args.last().is_some_and(|a| a == "/tmp/a.wav") || r.env.iter().any(|(_, v)| v == "/tmp/a.wav"));
        }
        let (a, b) = (temp_file("wav").unwrap(), temp_file("wav").unwrap());
        assert_ne!(a, b);
        assert_eq!(a.parent(), b.parent());
        // The folder is this user's alone, and its name can't be guessed.
        let dir = a.parent().unwrap();
        assert!(dir.is_dir() && dir.starts_with(std::env::temp_dir()));
        assert!(dir.file_name().unwrap().to_string_lossy().len() > "wordcraft-speech-1-".len() + 15);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(dir).unwrap().permissions().mode() & 0o777, 0o700);
        }
        assert_eq!(length_scale(f32::NAN), 1.0);
        assert_eq!(length_scale(100.0), 0.25);
    }
}
