//! ChatGPT Responses wire format. No partial output is eligible for execution.
use crate::{Endpoint, HttpRequest, Method, Msg, ToolCall, wire::Reply};
use serde_json::{Value, json};

pub const BASE: &str = "https://api.openai.com/v1";
pub const USAGE: &str = "https://chatgpt.com/settings/usage";
pub const LIMIT: &str =
    "ChatGPT usage limit reached or usage unavailable. Review Manage usage in ChatGPT settings. No other provider or billing method was used.";

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub slug: String,
    pub display_name: String,
}

pub fn models(v: &Value) -> Result<Vec<Model>, String> {
    let all = v.get("models").and_then(Value::as_array).ok_or("ChatGPT returned no model catalog.")?;
    Ok(all
        .iter()
        .filter(|m| m.get("visibility").and_then(Value::as_str) == Some("list"))
        .filter_map(|m| {
            let slug = m.get("slug")?.as_str()?;
            if slug.is_empty() || slug.len() > 200 {
                return None;
            }
            let display_name = m.get("display_name").and_then(Value::as_str).unwrap_or(slug).chars().take(200).collect();
            Some(Model { slug: slug.into(), display_name })
        })
        .take(1000)
        .collect())
}

pub fn build(ep: &Endpoint, system: &str, msgs: &[Msg], tools: &[Value]) -> HttpRequest {
    let mut input = Vec::new();
    for m in msgs {
        match m {
            Msg::User(t) => input.push(json!({"role":"user", "content":t})),
            Msg::ResponseItems(items) => input.extend(items.clone()),
            Msg::Assistant { text, calls } => {
                if !text.is_empty() {
                    input.push(json!({"role":"assistant", "content":text}));
                }
                input.extend(
                    calls
                        .iter()
                        .map(|c| json!({"type":"function_call","call_id":c.id,"namespace":"wordcraft","name":c.name,"arguments":c.args.to_string()})),
                );
            }
            Msg::Tools(outs) => input.extend(outs.iter().map(|o| json!({"type":"function_call_output","call_id":o.id,"output":o.text}))),
        }
    }
    let tools: Vec<Value> = tools
        .iter()
        .map(|t| json!({"type":"function","name":t.get("name"),"description":t.get("description"),"parameters":t.get("inputSchema"),"strict":false}))
        .collect();
    HttpRequest {
        method: Method::Post,
        url: format!("{BASE}/responses"),
        headers: vec![("content-type".into(), "application/json".into())],
        body: json!({
            "model":ep.model,"instructions":system,"input":input,"tools":[{"type":"namespace","name":"wordcraft","description":"Read and edit the open WordCraft document under the user-approved safeguards.","tools":tools}],
            "store":false,"stream":true,"include":["reasoning.encrypted_content"]
        })
        .to_string(),
    }
}

/// Deliberately never show server error messages: they may echo tokens or document text.
pub fn failure(v: &Value) -> String {
    let code = v.pointer("/error/code").or_else(|| v.pointer("/response/error/code")).or_else(|| v.get("code")).and_then(Value::as_str).unwrap_or("");
    match code {
        "subscription_sharing_usage_limit_exceeded" | "subscription_sharing_usage_unavailable" | "insufficient_quota" | "rate_limit_exceeded" => {
            LIMIT.into()
        }
        _ => "ChatGPT request failed. Check the connection and Manage usage; no alternate billing was used.".into(),
    }
}

/// Incremental SSE decoder. The host bounds the stream to 8 MiB, including unknown events.
#[derive(Default)]
pub struct Stream {
    data: String,
    done: std::collections::BTreeMap<u64, Value>,
    terminal: Option<Result<String, String>>,
}
impl Stream {
    pub fn line(&mut self, line: &str) -> Result<(), String> {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            if !self.data.is_empty() {
                let v: Value = serde_json::from_str(&self.data).map_err(|_| "Invalid ChatGPT stream.")?;
                self.data.clear();
                match v.get("type").and_then(Value::as_str) {
                    Some("response.output_item.done") => {
                        if self.terminal.is_some() {
                            return Err("Output arrived after ChatGPT completion.".into());
                        }
                        let index = v.get("output_index").and_then(Value::as_u64).filter(|i| *i < 1000).ok_or("Invalid ChatGPT output index.")?;
                        let item = v.get("item").filter(|v| v.is_object()).ok_or("Missing completed ChatGPT output item.")?;
                        if self.done.insert(index, item.clone()).is_some() {
                            return Err("Duplicate ChatGPT output item.".into());
                        }
                    }
                    Some("response.completed") => {
                        if self.terminal.is_some() {
                            return Err("Unexpected terminal event.".into());
                        }
                        let mut response = v.get("response").cloned().ok_or("Missing completed response.")?;
                        let output = response.get("output").and_then(Value::as_array).ok_or("Missing completed ChatGPT output.")?;
                        // This OAuth route sends completed items separately and an empty terminal output.
                        if output.is_empty() && !self.done.is_empty() {
                            if self.done.keys().enumerate().any(|(n, i)| n as u64 != *i) {
                                return Err("Missing ChatGPT output item.".into());
                            }
                            response["output"] = Value::Array(self.done.values().cloned().collect());
                        }
                        self.terminal = Some(Ok(response.to_string()));
                    }
                    Some("response.failed" | "error") => self.terminal = Some(Err(failure(&v))),
                    Some("response.incomplete") => {
                        self.terminal = Some(Err("ChatGPT response was incomplete; its tool calls were discarded.".into()))
                    }
                    _ => {}
                }
            }
        } else if let Some(data) = line.strip_prefix("data:") {
            if self.data.len().saturating_add(data.len()) > 8 * 1024 * 1024 {
                return Err("ChatGPT event is too large.".into());
            }
            if !self.data.is_empty() {
                self.data.push('\n');
            }
            self.data.push_str(data.strip_prefix(' ').unwrap_or(data));
        }
        Ok(())
    }
    pub fn finish(self) -> Result<String, String> {
        if !self.data.is_empty() {
            return Err("ChatGPT stream ended mid-event.".into());
        }
        self.terminal.unwrap_or_else(|| Err("ChatGPT stream ended without completion; its tool calls were discarded.".into()))
    }
}

pub fn parse(body: &str) -> Result<Reply, String> {
    let v: Value = serde_json::from_str(body).map_err(|_| "Invalid ChatGPT response.")?;
    if v.get("status").and_then(Value::as_str) != Some("completed") {
        return Err(failure(&v));
    }
    let items = v.get("output").and_then(Value::as_array).ok_or("ChatGPT response has no output.")?;
    if items.len() > 1000 {
        return Err("Too many ChatGPT output items.".into());
    }
    let mut reply = Reply { items: items.clone(), ..Default::default() };
    let mut ids = std::collections::BTreeSet::new();
    for item in items {
        match item.get("type").and_then(Value::as_str) {
            Some("message") => {
                if item.get("status").and_then(Value::as_str) != Some("completed") {
                    return Err("Incomplete ChatGPT message.".into());
                }
                for c in item.get("content").and_then(Value::as_array).into_iter().flatten() {
                    let field = match c.get("type").and_then(Value::as_str) {
                        Some("output_text") => "text",
                        Some("refusal") => "refusal",
                        _ => continue,
                    };
                    if !reply.text.is_empty() {
                        reply.text.push('\n');
                    }
                    reply.text.push_str(c.get(field).and_then(Value::as_str).ok_or("Invalid ChatGPT text.")?);
                }
            }
            Some("function_call") => {
                let field = |key| item.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).ok_or("Invalid ChatGPT tool call.");
                if field("status")? != "completed" {
                    return Err("Incomplete ChatGPT tool call.".into());
                }
                if item.get("namespace").and_then(Value::as_str).is_some_and(|n| n != "wordcraft") {
                    return Err("Unknown ChatGPT tool namespace.".into());
                }
                let id = field("call_id")?;
                let name = field("name")?;
                if !ids.insert(id) || !crate::tool_definitions().iter().any(|t| t.get("name").and_then(Value::as_str) == Some(name)) {
                    return Err("Invalid ChatGPT tool name or duplicate call id.".into());
                }
                let args: Value = serde_json::from_str(field("arguments")?).map_err(|_| "Invalid ChatGPT tool arguments.")?;
                if !args.is_object() {
                    return Err("ChatGPT tool arguments must be an object.".into());
                }
                reply.calls.push(ToolCall { id: id.into(), name: name.into(), args });
            }
            Some("reasoning") => {}
            _ => return Err("Unsupported ChatGPT output item.".into()),
        }
    }
    if reply.text.is_empty() && reply.calls.is_empty() {
        return Err("ChatGPT completed without text or tool calls. No edits were made.".into());
    }
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn completed(output: Value) -> Value {
        json!({"type":"response.completed","response":{"status":"completed","output":output}})
    }
    fn stream(v: &Value) -> Result<Reply, String> {
        let mut s = Stream::default();
        s.line(&format!("data: {v}\r\n"))?;
        s.line("\r\n")?;
        parse(&s.finish()?)
    }
    #[test]
    fn text_and_complete_tools_only() {
        let call = json!({"type":"function_call","status":"completed","call_id":"a","name":"get_text","arguments":"{}"});
        let mut output = json!([{"type":"message","status":"completed","content":[{"type":"output_text","text":"Hi"}]}, call]);
        let reply = stream(&completed(output.clone())).unwrap();
        assert_eq!(reply.text, "Hi");
        assert_eq!(reply.calls[0].name, "get_text");
        for args in ["{", "[]", "null"] {
            output[1]["arguments"] = json!(args);
            assert!(stream(&completed(output.clone())).is_err());
        }
        output[1]["arguments"] = json!("{}");
        output[1]["status"] = json!("in_progress");
        assert!(stream(&completed(output)).is_err());
    }
    #[test]
    fn interrupted_and_quota_streams_never_succeed_or_echo_secrets() {
        let mut s = Stream::default();
        s.line("data: {\"type\":\"response.function_call_arguments.delta\",\"delta\":\"secret\"}").unwrap();
        s.line("").unwrap();
        assert!(s.finish().is_err());
        for t in ["response.failed", "error"] {
            let v = json!({"type":t,"error":{"code":"subscription_sharing_usage_limit_exceeded","message":"secret"}});
            let e = stream(&v).unwrap_err();
            assert_eq!(e, LIMIT);
            assert!(!e.contains("secret"));
        }
        assert_eq!(stream(&json!({"type":"error","code":"subscription_sharing_usage_limit_exceeded","message":"secret"})).unwrap_err(), LIMIT);
        assert!(stream(&json!({"type":"response.incomplete"})).is_err());
    }
    #[test]
    fn request_replays_reasoning_and_never_stores_or_uses_api_key() {
        let ep = Endpoint { protocol: crate::Protocol::Responses, base_url: "http://evil".into(), model: "chosen".into(), api_key: "secret".into() };
        let item = json!({"type":"reasoning","encrypted_content":"opaque"});
        let req = build(&ep, "system", &[Msg::ResponseItems(vec![item.clone()])], &crate::tool_definitions());
        let v: Value = serde_json::from_str(&req.body).unwrap();
        assert_eq!(v["store"], false);
        assert_eq!(v["stream"], true);
        assert_eq!(v["input"][0], item);
        assert!(v.get("max_output_tokens").is_none());
        assert_eq!(v["tools"][0]["type"], "namespace");
        assert_eq!(v["tools"][0]["name"], "wordcraft");
        assert!(v["tools"][0]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "get_text"));
        assert_eq!(req.url, format!("{BASE}/responses"));
        assert!(!req.body.contains("secret"));
        assert_eq!(req.headers.len(), 1);
    }
    #[test]
    fn completed_stream_keeps_done_items_when_terminal_output_is_empty() {
        let item = json!({"type":"message","id":"message","status":"completed","content":[{"type":"output_text","text":"hello"}]});
        let mut s = Stream::default();
        s.line(&format!("data: {}", json!({"type":"response.output_item.done","output_index":0,"item":item}))).unwrap();
        s.line("").unwrap();
        s.line(&format!("data: {}", completed(json!([])))).unwrap();
        s.line("").unwrap();
        let reply = parse(&s.finish().unwrap()).unwrap();
        assert_eq!(reply.text, "hello");
        assert_eq!(reply.items, vec![item]);
    }
    #[test]
    fn done_tools_never_run_without_successful_terminal_completion() {
        let item = json!({"type":"function_call","namespace":"wordcraft","status":"completed","call_id":"read","name":"get_text","arguments":"{}"});
        for terminal in [
            None,
            Some(json!({"type":"response.failed","response":{"error":{"code":"subscription_sharing_usage_limit_exceeded"}}})),
            Some(json!({"type":"response.incomplete"})),
        ] {
            let mut s = Stream::default();
            s.line(&format!("data: {}", json!({"type":"response.output_item.done","output_index":0,"item":item}))).unwrap();
            s.line("").unwrap();
            if let Some(t) = terminal {
                s.line(&format!("data: {t}")).unwrap();
                s.line("").unwrap();
            }
            assert!(s.finish().is_err());
        }
        assert!(parse(&json!({"status":"completed","output":[]}).to_string()).is_err());
    }
    #[test]
    fn account_models_keep_order_and_visible_names() {
        let v = json!({"models":[{"slug":"b","display_name":"Bee","visibility":"list"},{"slug":"hidden","visibility":"hide"},{"slug":"a","display_name":"Ay","visibility":"list"}]});
        let m = models(&v).unwrap();
        assert_eq!(m[0].slug, "b");
        assert_eq!(m[0].display_name, "Bee");
        assert_eq!(m[1].slug, "a");
    }
}
