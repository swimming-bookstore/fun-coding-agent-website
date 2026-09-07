use gloo_net::http::Request;
use gloo_timers::future::TimeoutFuture;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use wasm_bindgen::JsCast;
use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Url};

pub use crate::vsh::Vfs;

const AUTH_KEY: &str = "fun-wasm-auth";
const XAI_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const XAI_SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";
const MODEL: &str = "grok-4-1-fast-non-reasoning";
const MAX_ROUNDS: usize = 12;

fn origin() -> String {
    web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_else(|| "http://127.0.0.1:8080".into())
}

fn device_url() -> String {
    format!("{}/xai-auth/oauth2/device/code", origin())
}

fn token_url() -> String {
    format!("{}/xai-auth/oauth2/token", origin())
}

fn responses_url() -> String {
    format!("{}/xai-api/v1/responses", origin())
}

#[derive(Clone, Debug)]
pub struct ChatLine {
    pub kind: LineKind,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    User,
    Agent,
    Tool,
    Note,
}

#[derive(Clone, Serialize, Deserialize)]
struct Tokens {
    access: String,
    refresh: String,
    expires: u64,
}

pub fn logged_in() -> bool {
    load_tokens().is_some()
}

fn load_tokens() -> Option<Tokens> {
    let w = web_sys::window()?;
    let s = w.local_storage().ok()??;
    let text = s.get_item(AUTH_KEY).ok()??;
    serde_json::from_str(&text).ok()
}

fn save_tokens(t: &Tokens) -> Result<(), String> {
    let w = web_sys::window().ok_or("no window")?;
    let s = w.local_storage().ok().flatten().ok_or("no storage")?;
    let text = serde_json::to_string(t).map_err(|e| e.to_string())?;
    s.set_item(AUTH_KEY, &text).map_err(|_| "storage".to_string())
}

pub async fn proxy_ready() -> bool {
    let url = format!("{}/xai-auth/", origin());
    match Request::get(&url).send().await {
        Ok(resp) => {
            let ctype = resp
                .headers()
                .get("content-type")
                .unwrap_or_default()
                .to_ascii_lowercase();
            !ctype.contains("text/html")
        }
        Err(_) => false,
    }
}

pub fn logout() {
    if let Some(w) = web_sys::window() {
        if let Ok(Some(s)) = w.local_storage() {
            let _ = s.remove_item(AUTH_KEY);
        }
    }
}

fn cors_hint(err: &str) -> String {
    let low = err.to_ascii_lowercase();
    if low.contains("failed to fetch") || low.contains("network") || low.contains("cors") {
        format!(
            "{err} — open this page via `trunk serve` or `fun-site` so /xai-auth and /xai-api can reach xAI. A file:// or GitHub Pages origin cannot."
        )
    } else {
        err.into()
    }
}

async fn post_form(url: &str, body: &str) -> Result<(u16, String), String> {
    let resp = Request::post(url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .map_err(|e| cors_hint(&e.to_string()))?
        .send()
        .await
        .map_err(|e| cors_hint(&e.to_string()))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    Ok((status, text))
}

#[derive(Clone)]
pub struct DeviceStart {
    pub user_code: String,
    pub uri: String,
    pub open_url: String,
    pub device_code: String,
    pub interval: u32,
    pub deadline_ms: f64,
}

pub async fn start_login() -> Result<DeviceStart, String> {
    let body = format!(
        "client_id={}&scope={}&referrer=connect",
        urlencoding(XAI_CLIENT_ID),
        urlencoding(XAI_SCOPE)
    );
    let (status, text) = post_form(&device_url(), &body).await?;
    if !(200..300).contains(&status) {
        return Err(format!("device auth HTTP {status}: {text}"));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let user_code = str_field(&v, "user_code")?;
    let uri = str_field(&v, "verification_uri")?;
    let open_url = v
        .get("verification_uri_complete")
        .and_then(|x| x.as_str())
        .unwrap_or(&uri)
        .to_string();
    let device_code = str_field(&v, "device_code")?;
    let interval = v.get("interval").and_then(|x| x.as_u64()).unwrap_or(5) as u32;
    let expires = v.get("expires_in").and_then(|x| x.as_u64()).unwrap_or(900) as f64;
    Ok(DeviceStart {
        user_code,
        uri,
        open_url,
        device_code,
        interval: interval.max(1),
        deadline_ms: js_sys::Date::now() + expires * 1000.0,
    })
}

pub async fn poll_login(start: &DeviceStart, gen: u64, live_gen: impl Fn() -> u64) -> Result<(), String> {
    loop {
        if live_gen() != gen {
            return Err("login cancelled".into());
        }
        if js_sys::Date::now() >= start.deadline_ms {
            return Err("login code expired".into());
        }
        TimeoutFuture::new(start.interval.saturating_mul(1000)).await;
        if live_gen() != gen {
            return Err("login cancelled".into());
        }
        let body = format!(
            "grant_type={}&client_id={}&device_code={}",
            urlencoding("urn:ietf:params:oauth:grant-type:device_code"),
            urlencoding(XAI_CLIENT_ID),
            urlencoding(&start.device_code)
        );
        let (status, text) = post_form(&token_url(), &body).await?;
        if (200..300).contains(&status) {
            if live_gen() != gen {
                return Err("login cancelled".into());
            }
            store_token_json(&text)?;
            return Ok(());
        }
        let err = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.get("error").and_then(|x| x.as_str()).map(|s| s.to_string()))
            .unwrap_or_default();
        match err.as_str() {
            "authorization_pending" | "slow_down" => {}
            "access_denied" | "authorization_denied" => return Err("login denied".into()),
            "expired_token" => return Err("login code expired".into()),
            _ => return Err(format!("token HTTP {status}: {text}")),
        }
    }
}

fn store_token_json(text: &str) -> Result<(), String> {
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let access = str_field(&v, "access_token")?;
    let mut refresh = v
        .get("refresh_token")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    if refresh.is_empty() {
        if let Some(old) = load_tokens() {
            refresh = old.refresh;
        }
    }
    let expires_in = v.get("expires_in").and_then(|x| x.as_u64()).unwrap_or(3600);
    save_tokens(&Tokens {
        access,
        refresh,
        expires: (js_sys::Date::now() as u64) + expires_in * 1000,
    })
}

async fn access_token() -> Result<String, String> {
    let t = load_tokens().ok_or_else(|| "not logged in".to_string())?;
    if (js_sys::Date::now() as u64) + 30_000 < t.expires {
        return Ok(t.access);
    }
    if t.refresh.is_empty() {
        logout();
        return Err("not logged in — token expired".into());
    }
    let body = format!(
        "grant_type=refresh_token&client_id={}&refresh_token={}",
        urlencoding(XAI_CLIENT_ID),
        urlencoding(&t.refresh)
    );
    let (status, text) = post_form(&token_url(), &body).await?;
    if !(200..300).contains(&status) {
        logout();
        return Err("not logged in — refresh failed".into());
    }
    store_token_json(&text)?;
    load_tokens()
        .map(|t| t.access)
        .ok_or_else(|| "not logged in".to_string())
}

fn urlencoding(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn str_field(v: &Value, key: &str) -> Result<String, String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("missing {key}"))
}

#[derive(Serialize)]
struct WireProp {
    #[serde(rename = "type")]
    kind: &'static str,
    description: &'static str,
}

#[derive(Serialize)]
struct WireParams {
    #[serde(rename = "type")]
    kind: &'static str,
    properties: BTreeMap<&'static str, WireProp>,
    required: Vec<&'static str>,
    #[serde(rename = "additionalProperties")]
    additional_properties: bool,
}

#[derive(Serialize)]
struct WireTool {
    #[serde(rename = "type")]
    kind: &'static str,
    name: &'static str,
    description: &'static str,
    parameters: WireParams,
}

fn tools() -> Vec<WireTool> {
    vec![
        tool(
            "read",
            "Read a file in the virtual workspace. A directory lists names.",
            &[("path", "string", "File path", true)],
        ),
        tool(
            "write",
            "Write a file in the virtual workspace.",
            &[
                ("path", "string", "File path", true),
                ("contents", "string", "Full contents", true),
            ],
        ),
        tool(
            "edit",
            "Replace old with new. old must appear exactly once.",
            &[
                ("path", "string", "File path", true),
                ("old", "string", "Exact text to find", true),
                ("new", "string", "Replacement", true),
            ],
        ),
        tool(
            "bash",
            "Virtual shell: ls, cat, pwd, mkdir, rm, echo, head, wc, python. Quotes, | and > work. python is RustPython in this tab (no pip). No real OS.",
            &[("cmd", "string", "Shell command", true)],
        ),
    ]
}

fn tool(
    name: &'static str,
    description: &'static str,
    fields: &[(&'static str, &'static str, &'static str, bool)],
) -> WireTool {
    let mut properties = BTreeMap::new();
    let mut required = Vec::new();
    for (n, ty, d, req) in fields {
        properties.insert(
            *n,
            WireProp {
                kind: ty,
                description: d,
            },
        );
        if *req {
            required.push(*n);
        }
    }
    WireTool {
        kind: "function",
        name,
        description,
        parameters: WireParams {
            kind: "object",
            properties,
            required,
            additional_properties: false,
        },
    }
}

#[derive(Deserialize)]
struct WireResponse {
    #[serde(default)]
    output: Vec<WireOutput>,
    output_text: Option<String>,
}

#[derive(Deserialize)]
struct WireOutput {
    #[serde(rename = "type")]
    kind: Option<String>,
    call_id: Option<String>,
    name: Option<String>,
    arguments: Option<String>,
    content: Option<Vec<WirePart>>,
}

#[derive(Deserialize)]
struct WirePart {
    text: Option<String>,
}

struct Reply {
    text: String,
    calls: Vec<(String, String, Value)>,
}

async fn complete(entries: &[Value]) -> Result<Reply, String> {
    let token = access_token().await?;
    let body = json!({
        "model": MODEL,
        "instructions": "You are Fun coding agent running in a browser. Tools: read, write, edit, bash.\nThe workspace is virtual — files exist only in this tab until the user downloads a zip.\nbash is a tiny virtual shell (ls, cat, pwd, mkdir, rm, echo, head, wc, python). Prefer write/edit.\nKeep replies short. Paths are relative to /workspace.",
        "input": entries,
        "tools": tools(),
        "stream": false,
    });
    let resp = Request::post(&responses_url())
        .header("Authorization", &format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(body.to_string())
        .map_err(|e| cors_hint(&e.to_string()))?
        .send()
        .await
        .map_err(|e| cors_hint(&e.to_string()))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status == 401 {
        logout();
        return Err("not logged in — token rejected".into());
    }
    if !(200..300).contains(&status) {
        return Err(format!("grok {status}: {text}"));
    }
    let wr: WireResponse = serde_json::from_str(&text).map_err(|e| format!("{e}: {text}"))?;
    let mut out = String::new();
    let mut calls = Vec::new();
    for item in wr.output {
        match item.kind.as_deref() {
            Some("function_call") => {
                let args = serde_json::from_str(item.arguments.as_deref().unwrap_or("{}"))
                    .unwrap_or(json!({}));
                calls.push((
                    item.call_id.unwrap_or_default(),
                    item.name.unwrap_or_default(),
                    args,
                ));
            }
            Some("message") => {
                if let Some(parts) = item.content {
                    for part in parts {
                        if let Some(t) = part.text {
                            out.push_str(&t);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if out.is_empty() {
        if let Some(t) = wr.output_text {
            out = t;
        }
    }
    Ok(Reply { text: out, calls })
}

fn run_tool(vfs: &mut Vfs, name: &str, args: &Value) -> String {
    let res = match name {
        "read" => vfs.read(args["path"].as_str().unwrap_or("")),
        "write" => vfs.write(
            args["path"].as_str().unwrap_or(""),
            args["contents"].as_str().unwrap_or(""),
        ),
        "edit" => vfs.edit(
            args["path"].as_str().unwrap_or(""),
            args["old"].as_str().unwrap_or(""),
            args["new"].as_str().unwrap_or(""),
        ),
        "bash" => vfs.bash(args["cmd"].as_str().unwrap_or("")),
        _ => Err(format!("unknown tool: {name}")),
    };
    match res {
        Ok(s) => s,
        Err(e) => e,
    }
}

fn args_repr(args: &Value) -> String {
    match args {
        Value::Object(map) if map.len() == 1 => map
            .values()
            .next()
            .and_then(|v| v.as_str())
            .unwrap_or(&args.to_string())
            .chars()
            .take(80)
            .collect(),
        _ => args.to_string().chars().take(80).collect(),
    }
}

pub async fn turn(
    vfs: &mut Vfs,
    history: &mut Vec<Value>,
    prompt: &str,
    lines: &mut Vec<ChatLine>,
) {
    history.push(json!({"role": "user", "content": prompt}));
    for _ in 0..MAX_ROUNDS {
        let reply = match complete(history).await {
            Ok(r) => r,
            Err(e) => {
                lines.push(ChatLine {
                    kind: LineKind::Note,
                    text: e,
                });
                return;
            }
        };
        if !reply.text.trim().is_empty() {
            lines.push(ChatLine {
                kind: LineKind::Agent,
                text: reply.text.clone(),
            });
        }
        if !reply.text.is_empty() {
            history.push(json!({"role": "assistant", "content": reply.text}));
        }
        if reply.calls.is_empty() {
            return;
        }
        for (id, name, args) in reply.calls {
            history.push(json!({
                "type": "function_call",
                "call_id": id,
                "name": name,
                "arguments": args.to_string(),
            }));
            let content = run_tool(vfs, &name, &args);
            lines.push(ChatLine {
                kind: LineKind::Tool,
                text: format!("{}({})", name, args_repr(&args)),
            });
            history.push(json!({
                "type": "function_call_output",
                "call_id": id,
                "output": content,
            }));
        }
    }
    lines.push(ChatLine {
        kind: LineKind::Note,
        text: "stopped after too many tool rounds".into(),
    });
}

pub fn download_zip(vfs: &Vfs) -> Result<(), String> {
    let bytes = vfs.zip()?;
    let uint8 = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
    uint8.copy_from(&bytes);
    let parts = js_sys::Array::new();
    parts.push(&uint8);
    let bag = BlobPropertyBag::new();
    bag.set_type("application/zip");
    let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &bag)
        .map_err(|_| "blob".to_string())?;
    let url = Url::create_object_url_with_blob(&blob).map_err(|_| "url".to_string())?;
    let doc = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("document")?;
    let a: HtmlAnchorElement = doc
        .create_element("a")
        .map_err(|_| "a")?
        .dyn_into()
        .map_err(|_| "a")?;
    a.set_href(&url);
    a.set_download("fun-demo.zip");
    a.click();
    let _ = Url::revoke_object_url(&url);
    Ok(())
}
