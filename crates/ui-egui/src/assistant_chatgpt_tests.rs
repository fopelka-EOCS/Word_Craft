//! Account/consent isolation tests use no network and never access Keychain.
use super::*;
use std::{cell::RefCell, collections::VecDeque, rc::Rc, sync::mpsc};
use wordcraft_assistant::{
    Http, HttpRequest,
    chatgpt::{Connection, Status},
};

struct FakeConnection {
    state: Rc<RefCell<Status>>,
    http: Box<Http>,
}
impl Connection for FakeConnection {
    fn status(&self) -> Status {
        self.state.borrow().clone()
    }
    fn sign_in(&mut self) {
        let mut s = self.state.borrow_mut();
        s.connected = true;
        s.generation += 1;
    }
    fn cancel(&mut self) {
        self.state.borrow_mut().busy = false;
    }
    fn disconnect(&mut self) {
        let mut s = self.state.borrow_mut();
        s.connected = false;
        s.generation += 1;
    }
    fn acknowledge_usage(&mut self) {
        self.state.borrow_mut().usage_acknowledged = true;
    }
    fn cancel_requests(&mut self) {}
    fn http(&self) -> &Http {
        self.http.as_ref()
    }
}
type Sent = Rc<RefCell<Vec<HttpRequest>>>;
type Replies = Rc<RefCell<VecDeque<mpsc::Sender<Result<String, String>>>>>;
fn app() -> (WordApp, Sent, Replies, Rc<RefCell<Status>>) {
    let sent: Sent = Rc::default();
    let replies: Replies = Rc::default();
    let (s, q) = (sent.clone(), replies.clone());
    let http = Box::new(move |req: HttpRequest| {
        s.borrow_mut().push(req);
        let (tx, rx) = mpsc::channel();
        q.borrow_mut().push_back(tx);
        rx
    });
    let state = Rc::new(RefCell::new(Status { connected: true, account: Some("writer@example.test".into()), ..Default::default() }));
    let services = crate::Services { chatgpt: Some(Box::new(FakeConnection { state: state.clone(), http })), ..Default::default() };
    let mut a = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), services);
    a.assistant.settings.provider = CHATGPT.into();
    a.assistant.settings.current_mut().model = "synthetic-model".into();
    a.session.author = "Human Writer".into();
    sync(&mut a);
    (a, sent, replies, state)
}
fn ready(a: &mut WordApp) {
    command(a, &json!({"chatgpt":"acknowledgeUsage"})).unwrap();
    command(a, &json!({"consent":true})).unwrap();
    let ep = a.assistant.settings.connection().unwrap();
    a.assistant.models.lists.insert(CHATGPT.into(), ModelList { key: cache_key(&ep), list: vec!["synthetic-model".into()], ..Default::default() });
}
fn tick(a: &mut WordApp) {
    poll(a, &egui::Context::default());
}
fn complete(items: Value) -> String {
    json!({"status":"completed","output":items}).to_string()
}
fn calls(items: &[(&str, &str, Value)]) -> String {
    complete(
        items
            .iter()
            .map(|(id, name, args)| json!({"type":"function_call","status":"completed","call_id":id,"name":name,"arguments":args.to_string()}))
            .collect(),
    )
}
fn answer(q: &Replies, body: String) {
    q.borrow_mut().pop_front().unwrap().send(Ok(body)).unwrap();
}

#[test]
fn no_document_request_before_usage_acknowledgment_and_document_consent() {
    let (mut a, sent, _, _) = app();
    a.session.run("text.insert", &json!({"text":"private synthetic document"})).unwrap();
    assert!(command(&mut a, &json!({"ask":"summarize"})).unwrap_err().contains("Got it"));
    command(&mut a, &json!({"chatgpt":"acknowledgeUsage"})).unwrap();
    assert!(command(&mut a, &json!({"ask":"summarize"})).unwrap_err().contains("Allow sending"));
    assert!(sent.borrow().is_empty());
    ready(&mut a);
    command(&mut a, &json!({"ask":"summarize"})).unwrap();
    assert_eq!(sent.borrow().len(), 1);
    let status = command(&mut a, &json!({"open":true})).unwrap();
    assert_eq!(status["documentConsent"], true);
    assert_eq!(status["chatgpt"]["account"], "writer@example.test");
    assert!(!status.to_string().contains("accessToken"));
}
#[test]
fn completed_response_edit_tracks_and_undo_restores_document() {
    let (mut a, _, q, _) = app();
    ready(&mut a);
    command(&mut a, &json!({"ask":"Write a greeting"})).unwrap();
    answer(&q, calls(&[("read", "get_text", json!({})), ("edit", "type_text", json!({"text":"Synthetic greeting"}))]));
    tick(&mut a);
    answer(&q, calls(&[("verify", "get_text", json!({}))]));
    tick(&mut a);
    answer(&q, complete(json!([{"type":"message","status":"completed","content":[{"type":"output_text","text":"Done"}]}])));
    tick(&mut a);
    assert!(!a.assistant.agent.busy());
    assert_eq!(a.session.author, "Human Writer");
    assert!(!a.session.doc.settings.track_changes);
    assert!(a.session.doc.plain_text(wordcraft_doc::StoryRef::Body).contains("Synthetic greeting"));

    assert_eq!(a.session.undo_depth(), 1);
    a.session.run("edit.undo", &json!({})).unwrap();
    assert!(!a.session.doc.settings.track_changes, "Undo must not restore the assistant's temporary tracking setting");
    assert!(!a.session.doc.plain_text(wordcraft_doc::StoryRef::Body).contains("Synthetic greeting"));
    a.session.redo();
    assert!(!a.session.doc.settings.track_changes, "Redo keeps the original tracking preference");
    a.session.run("review.rejectAll", &json!({})).unwrap();
    assert!(!a.session.doc.plain_text(wordcraft_doc::StoryRef::Body).contains("Synthetic greeting"));
}
#[test]
fn new_document_drops_late_results_consent_and_previous_context() {
    let (mut a, sent, q, _) = app();
    ready(&mut a);
    command(&mut a, &json!({"ask":"OLD PRIVATE MESSAGE"})).unwrap();
    let late = q.borrow_mut().pop_front().unwrap();
    a.execute("file.new", json!({})).unwrap();
    assert_eq!(a.session.author, "Human Writer");
    assert!(!a.session.doc.settings.track_changes);
    assert!(late.send(Ok(calls(&[("read", "get_text", json!({})), ("edit", "type_text", json!({"text":"STALE"}))]))).is_err());
    tick(&mut a);
    assert!(!a.assistant.agent.busy());
    assert!(!a.assistant.consent);
    assert!(!a.session.doc.plain_text(wordcraft_doc::StoryRef::Body).contains("STALE"));
    assert!(command(&mut a, &json!({"ask":"new request"})).is_err());
    ready(&mut a);
    command(&mut a, &json!({"ask":"new request"})).unwrap();
    assert!(!sent.borrow().last().unwrap().body.contains("OLD PRIVATE MESSAGE"));
}
#[test]
fn account_switch_and_disconnect_restore_temporary_author_and_tracking() {
    let (mut a, sent, q, state) = app();
    ready(&mut a);
    command(&mut a, &json!({"ask":"old account context"})).unwrap();
    state.borrow_mut().generation += 1;
    tick(&mut a);
    assert!(!a.assistant.agent.busy());
    assert!(!a.assistant.consent);
    assert_eq!(a.session.author, "Human Writer");
    assert!(q.borrow_mut().pop_front().unwrap().send(Ok(complete(json!([])))).is_err());
    ready(&mut a);
    command(&mut a, &json!({"ask":"new account context"})).unwrap();
    assert!(!sent.borrow().last().unwrap().body.contains("old account context"));
    command(&mut a, &json!({"chatgpt":"disconnect"})).unwrap();
    assert!(!a.assistant.consent);
    assert_eq!(a.session.author, "Human Writer");
    assert!(!a.session.doc.settings.track_changes);
}

#[test]
fn open_restores_the_original_author_and_tracking_before_loading() {
    let (mut a, _, q, _) = app();
    ready(&mut a);
    command(&mut a, &json!({"ask":"old context"})).unwrap();
    let path = std::env::temp_dir().join(format!("wordcraft-oauth-open-{}.txt", std::process::id()));
    std::fs::write(&path, "A synthetic replacement document.").unwrap();
    a.execute("file.open", json!({"path":path.to_string_lossy()})).unwrap();
    std::fs::remove_file(path).unwrap();
    tick(&mut a);
    assert_eq!(a.session.author, "Human Writer");
    assert!(!a.session.doc.settings.track_changes);
    assert!(!a.assistant.consent);
    assert!(a.assistant.agent.log.is_empty());
    assert!(q.borrow_mut().pop_front().unwrap().send(Ok(complete(json!([])))).is_err());
}

#[test]
fn chatgpt_controls_explain_usage_and_document_consent() {
    use egui_kittest::kittest::Queryable;
    let (mut a, _, _, _) = app();
    a.assistant.open = true;
    a.assistant.settings_open = true;
    let mut builder = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 920.0)).with_pixels_per_point(2.0);
    if std::env::var_os("WORDCRAFT_OAUTH_UI_SHOT").is_some() {
        builder = builder.wgpu();
    }
    let mut h = builder.build_ui_state(
        |ui, app: &mut WordApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        a,
    );
    h.run_steps(6);
    assert!(h.query_by_label("Got it").is_some());
    assert!(h.query_by_label("Manage usage").is_some());
    if let Ok(path) = std::env::var("WORDCRAFT_OAUTH_UI_SHOT") {
        h.render().unwrap().save(&path).unwrap();
    }
    h.get_by_label("Got it").click();
    h.run_steps(6);
    assert!(h.query_by_label("Allow document access").is_some());
    if let Ok(path) = std::env::var("WORDCRAFT_OAUTH_UI_SHOT") {
        h.render().unwrap().save(format!("{path}.consent.png")).unwrap();
    }
}
