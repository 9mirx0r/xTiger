//! The Model Context Protocol over stdio: one JSON-RPC message per line.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::{Map, Value, json};

use crate::journal::{self, Entry, ModRef, Outcome};
use crate::locate::Locations;
use crate::sessions::{self, Tracker};
use crate::tools::{self, CallError, Content, Context};

/// The protocol versions this server speaks, newest first.
pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "xTiger checks Crusader Kings III mods. Typical work: xtiger_validate the mod (by name; \
xtiger_mods lists them), look at the shape with xtiger_reports group_by=key or group_by=file, then list the \
reports of one key or file, open the file at the given line, fix it, and validate again: since_last_run says \
what was fixed and what is new. Errors and fatal reports first; tips and untidy last. Use ck3_run to test \
events and effects in the real game (Windows), and ck3_logs to read the game's own logs. If something is \
not found, xtiger_status says what was looked at. Always pass reason: one short sentence on why you make the \
call, such as \"Checking that the fixed event loads\". The user follows your work in the xTiger app through it. \
At the start of a job, call xtiger_session with a title such as \"Update Better Courts to 1.20\"; at the end, \
call it again with wrap_up: what you did and what is left. The user can also leave requests for you in the \
app, such as fixing a report or updating a mod: xtiger_status says how many are waiting, \
xtiger_pending_requests lists them, and xtiger_finish_request closes each one. To see how the game does \
something, ck3_vanilla finds its definitions in the game's files and ck3_docs looks up triggers, effects and \
modifiers in the game's own documentation. xtiger_compare tells what changed between two checks. When a mod is moved to a newer game version, \
xtiger_overrides shows which game files and definitions the mod replaces and how each copy differs from the \
game now, and xtiger_migrate lists the mechanical renames it still needs (a dry run, nothing is written); xtiger_migrate_apply writes them, after a backup. After a play-test, xtiger_game_gap shows what the game's error.log said about the mod's files that Tiger did not report.";

const INVALID_PARAMS: i64 = -32602;
const METHOD_NOT_FOUND: i64 = -32601;
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;

type Output = Arc<Mutex<Box<dyn Write + Send>>>;
/// The assistant's name, as it gave it in `initialize`.
type Client = Arc<Mutex<Option<String>>>;
/// The work session of this server.
type Sessions = Arc<Mutex<Tracker>>;
pub type Detect = Arc<dyn Fn() -> Locations + Send + Sync>;

fn send(out: &Output, message: &Value) {
    let mut line = serde_json::to_string(message).unwrap_or_default();
    line.push('\n');
    if let Ok(mut out) = out.lock() {
        let _ = out.write_all(line.as_bytes());
        let _ = out.flush();
    }
}

#[allow(clippy::needless_pass_by_value)] // Callers build the result in place.
fn reply(out: &Output, id: &Value, result: Value) {
    send(out, &json!({"jsonrpc": "2.0", "id": id, "result": result}));
}

fn error(out: &Output, id: &Value, code: i64, message: &str) {
    send(out, &json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}));
}

fn initialize(params: &Value, client: &Client) -> Value {
    let name = params
        .pointer("/clientInfo/title")
        .or_else(|| params.pointer("/clientInfo/name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty());
    if let (Some(name), Ok(mut client)) = (name, client.lock()) {
        *client = Some(name.chars().take(60).collect());
    }
    let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
    let version = PROTOCOL_VERSIONS.iter().find(|v| **v == asked).unwrap_or(&PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": {"tools": {}, "prompts": {}},
        "serverInfo": {"name": "xtiger", "title": "xTiger", "version": env!("CARGO_PKG_VERSION")},
        "instructions": INSTRUCTIONS,
    })
}

struct Prompt {
    name: &'static str,
    title: &'static str,
    description: &'static str,
    text: &'static str,
}

const PROMPTS: [Prompt; 2] = [
    Prompt {
        name: "fix_mod",
        title: "Fix a mod's problems",
        description: "Validate a mod and fix its problems, most serious first.",
        text: "Validate the CK3 mod \"{mod}\" with xtiger_validate. Then use xtiger_reports with group_by=key to \
               see which kinds of problems there are. Fix them most serious first (fatal, error, warning; leave \
               untidy and tips for last): list the reports of one key, open each file at the given line, \
               understand the cause, and fix it in the mod's files. Never edit the base game. After each batch, \
               validate again and check since_last_run to confirm the fixes and that nothing new appeared. When \
               a report is a false positive, say so instead of changing the mod to silence it.",
    },
    Prompt {
        name: "update_mod",
        title: "Update a mod to the current game",
        description: "Find what a game update broke in a mod and fix it.",
        text: "The CK3 mod \"{mod}\" was made for an older version of the game. Use xtiger_status to see the \
               game version, then xtiger_validate the mod. Look for problems caused by the update: unknown \
               triggers, effects, modifiers or scopes, renamed or removed files, and changed formats \
               (xtiger_reports group_by=key, then the keys missing-item, unknown-field, removed-item and \
               validation). Fix them in the mod's files, update supported_version in its descriptor, and \
               validate again until no update problems are left. Summarize what changed.",
    },
];

fn prompts_list() -> Value {
    let prompts: Vec<Value> = PROMPTS
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "title": p.title,
                "description": p.description,
                "arguments": [{"name": "mod", "description": "The mod's name, folder or .mod file.", "required": true}],
            })
        })
        .collect();
    json!({ "prompts": prompts })
}

fn prompts_get(params: &Value) -> Result<Value, String> {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let prompt = PROMPTS
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| format!("No prompt called {name}"))?;
    let wanted = params.pointer("/arguments/mod").and_then(Value::as_str).unwrap_or("").trim();
    if wanted.is_empty() {
        return Err("The prompt needs the mod argument.".to_owned());
    }
    Ok(json!({
        "description": prompt.description,
        "messages": [{"role": "user", "content": {"type": "text", "text": prompt.text.replace("{mod}", wanted)}}],
    }))
}

fn content_json(content: Vec<Content>) -> Vec<Value> {
    content
        .into_iter()
        .map(|item| match item {
            Content::Text(text) => json!({"type": "text", "text": text}),
            Content::Image(path) => match fs::read(&path) {
                Ok(bytes) => json!({
                    "type": "image",
                    "data": base64::engine::general_purpose::STANDARD.encode(bytes),
                    "mimeType": "image/png",
                }),
                Err(e) => json!({"type": "text", "text": format!("(the screenshot could not be read: {e})")}),
            },
        })
        .collect()
}

/// Sends `notifications/progress` for a call whose client asked for them.
#[derive(Clone)]
struct Progress {
    out: Output,
    token: Value,
    count: Arc<AtomicU64>,
}

impl Progress {
    fn send(&self, message: &str) {
        let n = self.count.fetch_add(1, Ordering::SeqCst) + 1;
        send(
            &self.out,
            &json!({"jsonrpc": "2.0", "method": "notifications/progress",
                    "params": {"progressToken": self.token, "progress": n, "message": message}}),
        );
    }
}

struct Running {
    cancelled: Arc<AtomicBool>,
    /// Set when the client cancelled the request, which then gets no answer.
    silenced: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

/// Serve requests from `input` until it ends. `detect` finds the game, the validator and the
/// folders; it runs again for every tool call.
pub fn serve<R: BufRead>(input: R, out: Box<dyn Write + Send>, detect: &Detect) {
    let out: Output = Arc::new(Mutex::new(out));
    let client: Client = Arc::default();
    let sessions: Sessions = Arc::default();
    let mut running: HashMap<String, Running> = HashMap::new();
    for line in input.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        running.retain(|_, call| !call.handle.is_finished());
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(e) => {
                error(&out, &Value::Null, PARSE_ERROR, &format!("Parse error: {e}"));
                continue;
            }
        };
        // Batches were allowed by one version of the protocol; answer each message in turn.
        let messages = match message {
            Value::Array(messages) => messages,
            message => vec![message],
        };
        for message in messages {
            handle(&message, &out, detect, (&client, &sessions), &mut running);
        }
    }
    // The input ended: stop whatever is still running, so no validator or game is left behind. Each
    // call still answers, in case the client only closed its side.
    for call in running.values() {
        call.cancelled.store(true, Ordering::SeqCst);
    }
    for (_, call) in running {
        let _ = call.handle.join();
    }
}

fn handle(
    message: &Value,
    out: &Output,
    detect: &Detect,
    (client, sessions): (&Client, &Sessions),
    running: &mut HashMap<String, Running>,
) {
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // A response to a request of ours (there are none) or garbage.
        if let Some(id) = message
            .get("id")
            .filter(|_| message.get("result").is_none() && message.get("error").is_none())
        {
            error(out, id, INVALID_REQUEST, "Invalid request: no method");
        }
        return;
    };
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let Some(id) = message.get("id").cloned() else {
        // A notification.
        if method == "notifications/cancelled"
            && let Some(request) = params.get("requestId")
            && let Some(call) = running.get(&request.to_string())
        {
            call.silenced.store(true, Ordering::SeqCst);
            call.cancelled.store(true, Ordering::SeqCst);
        }
        return;
    };
    match method {
        "initialize" => reply(out, &id, initialize(&params, client)),
        "ping" => reply(out, &id, json!({})),
        "tools/list" => reply(out, &id, tools::list()),
        "prompts/list" => reply(out, &id, prompts_list()),
        "prompts/get" => match prompts_get(&params) {
            Ok(result) => reply(out, &id, result),
            Err(message) => error(out, &id, INVALID_PARAMS, &message),
        },
        "resources/list" => reply(out, &id, json!({"resources": []})),
        "tools/call" => {
            let cancelled = Arc::new(AtomicBool::new(false));
            let silenced = Arc::new(AtomicBool::new(false));
            let flags = (Arc::clone(&cancelled), Arc::clone(&silenced));
            let client = client.lock().ok().and_then(|client| client.clone());
            let call = Call {
                id: id.clone(),
                params,
                out: Arc::clone(out),
                client,
                sessions: Arc::clone(sessions),
            };
            let handle = start_call(call, Arc::clone(detect), flags);
            running.insert(id.to_string(), Running { cancelled, silenced, handle });
        }
        _ => error(out, &id, METHOD_NOT_FOUND, &format!("Method not found: {method}")),
    }
}

/// How often a long call reports that it is still working, so clients do not give up on it.
const HEARTBEAT: Duration = Duration::from_secs(10);

/// A `tools/call` request.
struct Call {
    id: Value,
    params: Value,
    out: Output,
    client: Option<String>,
    sessions: Sessions,
}

/// The session a call that starts at `now` belongs to. `xtiger_session` can name the job or wrap it up.
fn join_session(
    sessions: &Sessions,
    name: &str,
    arguments: &Map<String, Value>,
    now: u64,
) -> String {
    let job = |field: &str, max: usize| {
        (name == sessions::TOOL).then(|| journal::clean_line(arguments.get(field), max)).flatten()
    };
    let title = job("title", sessions::MAX_TITLE);
    let wrap_up = job("wrap_up", sessions::MAX_WRAP_UP).is_some();
    let Ok(mut tracker) = sessions.lock() else { return String::new() };
    let session = tracker.join(now, title.as_deref());
    if wrap_up {
        tracker.wrap_up();
    }
    session
}

/// A journal id that no other call of any server shares.
fn entry_id() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    format!("{}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::SeqCst) + 1)
}

fn start_call(
    call: Call,
    detect: Detect,
    (cancelled, silenced): (Arc<AtomicBool>, Arc<AtomicBool>),
) -> JoinHandle<()> {
    thread::spawn(move || {
        let Call { id, params, out, client, sessions } = call;
        let name = params.get("name").and_then(Value::as_str).unwrap_or("").to_owned();
        let empty = Map::new();
        let arguments = match params.get("arguments") {
            None | Some(Value::Null) => &empty,
            Some(Value::Object(arguments)) => arguments,
            Some(_) => {
                error(&out, &id, INVALID_PARAMS, "arguments must be an object");
                return;
            }
        };
        let progress = params
            .pointer("/_meta/progressToken")
            .filter(|token| token.is_string() || token.is_number())
            .map(|token| Progress {
                out: Arc::clone(&out),
                token: token.clone(),
                count: Arc::new(AtomicU64::new(0)),
            });
        let loc = detect();
        let started_at = journal::now_ms();
        let known = tools::exists(&name);
        let session = if known {
            join_session(&sessions, &name, arguments, started_at)
        } else {
            String::new()
        };

        // Calls to tools that exist are shown in the app while they run, then kept in the journal.
        let journal = known.then(|| {
            let entry = Entry {
                id: entry_id(),
                tool: name.clone(),
                client: client.clone(),
                reason: journal::clean_reason(arguments.get(tools::REASON)),
                mod_ref: None,
                args: journal::brief_args(arguments),
                started_at,
                finished_at: None,
                duration_ms: None,
                outcome: None,
                summary: None,
                progress: None,
                run_id: None,
                found: None,
                session: Some(session.clone()),
            };
            Arc::new(Mutex::new(journal::Running::start(&loc.state_dir, entry)))
        });
        let latest: Arc<Mutex<Option<String>>> = Arc::default();

        let done = Arc::new(AtomicBool::new(false));
        let heartbeat = (progress.is_some() || journal.is_some()).then(|| {
            let done = Arc::clone(&done);
            let progress = progress.clone();
            let journal = journal.clone();
            let latest = Arc::clone(&latest);
            thread::spawn(move || {
                let started = Instant::now();
                let mut last_beat = Instant::now();
                let mut last_touch = Instant::now();
                while !done.load(Ordering::SeqCst) {
                    thread::sleep(Duration::from_millis(250));
                    if let Some(progress) = &progress
                        && last_beat.elapsed() >= HEARTBEAT
                    {
                        last_beat = Instant::now();
                        progress
                            .send(&format!("Still working ({} s)", started.elapsed().as_secs()));
                    }
                    if let Some(journal) = &journal
                        && last_touch.elapsed() >= journal::TOUCH
                    {
                        last_touch = Instant::now();
                        let line = latest.lock().ok().and_then(|mut line| line.take());
                        if let Ok(mut running) = journal.lock() {
                            running.touch(line.as_deref());
                        }
                    }
                }
            })
        });

        let mut on_line = |line: &str| {
            if let Some(progress) = &progress {
                progress.send(line);
            }
            if let Ok(mut latest) = latest.lock() {
                *latest = Some(line.to_owned());
            }
        };
        let mut working_on = |mod_ref: &ModRef| {
            if let Some(Ok(mut running)) = journal.as_ref().map(|journal| journal.lock()) {
                running.entry.mod_ref = Some(mod_ref.clone());
                running.touch(None);
            }
        };
        let is_cancelled = || cancelled.load(Ordering::SeqCst);
        let mut ctx = Context {
            progress: &mut on_line,
            cancelled: &is_cancelled,
            working_on: &mut working_on,
            session: &session,
            client: client.as_deref(),
        };
        let result = tools::call(&name, arguments, &loc, &mut ctx);
        done.store(true, Ordering::SeqCst);
        if known && let Ok(mut tracker) = sessions.lock() {
            tracker.seen(journal::now_ms());
        }
        if let Some(heartbeat) = heartbeat {
            let _ = heartbeat.join();
        }

        if let Some(running) =
            journal.and_then(|journal| Arc::into_inner(journal)?.into_inner().ok())
        {
            finish_entry(running, &result, is_cancelled());
        }

        // A request the client cancelled gets no answer.
        if silenced.load(Ordering::SeqCst) {
            return;
        }
        match result {
            Ok(done) => {
                reply(&out, &id, json!({"content": content_json(done.content), "isError": false}));
            }
            Err(CallError::Failed(message)) => {
                reply(
                    &out,
                    &id,
                    json!({"content": [{"type": "text", "text": message}], "isError": true}),
                );
            }
            Err(CallError::Unknown(name)) => {
                error(&out, &id, INVALID_PARAMS, &format!("Unknown tool: {name}"));
            }
        }
    })
}

fn finish_entry(
    mut running: journal::Running,
    result: &Result<tools::Done, CallError>,
    cancelled: bool,
) {
    match result {
        Ok(done) => {
            if let Some(mod_ref) = &done.note.mod_ref {
                running.entry.mod_ref = Some(mod_ref.clone());
            }
            running.entry.run_id.clone_from(&done.note.run_id);
            running.entry.found = done.note.found;
            running.finish(Outcome::Ok, &done.note.summary);
        }
        Err(_) if cancelled => running.finish(Outcome::Cancelled, "Cancelled"),
        Err(CallError::Failed(message) | CallError::Unknown(message)) => {
            running.finish(Outcome::Error, message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locate::{AppSettings, Sources};
    use crate::testing::TempDir;
    use std::fmt::Write as _;
    use std::io::Cursor;
    use std::path::PathBuf;

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn detect_in(dir: PathBuf) -> Detect {
        Arc::new(move || {
            let mut env = HashMap::new();
            env.insert("CK3_USER_DIR".to_owned(), dir.join("user").display().to_string());
            env.insert("XTIGER_STATE_DIR".to_owned(), dir.join("state").display().to_string());
            Locations::from_sources(&Sources {
                env,
                exe_dir: None,
                app_data: None,
                app_settings: AppSettings::default(),
                installed_app: None,
                home: None,
                user_dir_candidates: vec![],
                steam: Box::new(|| None),
                on_path: Box::new(|_| None),
            })
        })
    }

    /// Feed `lines` to a server and return what it answered, by id, plus notifications.
    fn talk(lines: &[Value]) -> (HashMap<String, Value>, Vec<Value>) {
        talk_in(&TempDir::new(), lines)
    }

    fn talk_in(tmp: &TempDir, lines: &[Value]) -> (HashMap<String, Value>, Vec<Value>) {
        let mut input = String::new();
        for line in lines {
            let _ = writeln!(input, "{line}");
        }
        input.push_str("not json\n");
        let captured = Captured::default();
        serve(Cursor::new(input), Box::new(captured.clone()), &detect_in(tmp.to_path_buf()));
        let text = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
        let mut answers = HashMap::new();
        let mut notes = Vec::new();
        for line in text.lines() {
            let value: Value = serde_json::from_str(line).unwrap();
            match value.get("id") {
                Some(id) if !id.is_null() => {
                    answers.insert(id.to_string(), value);
                }
                _ => notes.push(value),
            }
        }
        (answers, notes)
    }

    #[test]
    fn handshake_tools_and_prompts() {
        let (answers, notes) = talk(&[
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26"}}),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
            json!({"jsonrpc": "2.0", "id": "p", "method": "prompts/get", "params": {"name": "fix_mod", "arguments": {"mod": "Silk"}}}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "nope"}),
            json!({"jsonrpc": "2.0", "id": 4, "method": "initialize", "params": {"protocolVersion": "1999-01-01"}}),
            json!({"jsonrpc": "2.0", "id": 5, "method": "ping"}),
        ]);
        assert_eq!(answers["1"]["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(answers["1"]["result"]["serverInfo"]["name"], "xtiger");
        assert_eq!(answers["4"]["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
        let instructions = answers["1"]["result"]["instructions"].as_str().unwrap();
        assert!(instructions.contains("reason") && instructions.contains(sessions::TOOL));
        let tools = answers["2"]["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 19);
        assert!(tools.iter().all(|tool| tool["inputSchema"]["properties"]["reason"].is_object()));
        let prompt = answers["\"p\""]["result"]["messages"][0]["content"]["text"].as_str().unwrap();
        assert!(prompt.contains("\"Silk\""));
        assert_eq!(answers["3"]["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(answers["5"]["result"], json!({}));
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0]["error"]["code"], PARSE_ERROR);
    }

    #[test]
    fn tool_calls_and_errors() {
        let (answers, _) = talk(&[
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "xtiger_status"}}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "xtiger_reports", "arguments": {}}}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "nope", "arguments": {}}}),
            json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "ck3_logs", "arguments": []}}),
        ]);
        assert_eq!(answers["1"]["result"]["isError"], false);
        let status: Value =
            serde_json::from_str(answers["1"]["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(status["user_dir"]["from"], "CK3_USER_DIR");
        assert_eq!(answers["2"]["result"]["isError"], true);
        assert!(
            answers["2"]["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("No results yet")
        );
        assert_eq!(answers["3"]["error"]["code"], INVALID_PARAMS);
        assert_eq!(answers["4"]["error"]["code"], INVALID_PARAMS);
    }

    #[test]
    fn calls_are_written_to_the_journal() {
        let tmp = TempDir::new();
        talk_in(
            &tmp,
            &[
                json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"clientInfo": {"name": "test-client", "title": "Test Client"}}}),
                json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "xtiger_status", "arguments": {"reason": "Seeing what is set up"}}}),
                json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "nope", "arguments": {}}}),
            ],
        );
        // Straight to the call: when the input ends, the server cancels calls that still run.
        let call = Call {
            id: json!(1),
            params: json!({"name": "xtiger_reports", "arguments": {"severity": "error"}}),
            out: Arc::new(Mutex::new(Box::new(Captured::default()))),
            client: None,
            sessions: Arc::default(),
        };
        let flags = (Arc::default(), Arc::default());
        start_call(call, detect_in(tmp.to_path_buf()), flags).join().unwrap();
        let state = tmp.join("state");
        let entries = journal::read_entries(&state, 10);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].tool, "xtiger_status");
        assert_eq!(entries[1].client.as_deref(), Some("Test Client"));
        assert_eq!(entries[1].reason.as_deref(), Some("Seeing what is set up"));
        assert_eq!(entries[1].outcome, Some(Outcome::Ok));
        assert!(entries[1].summary.as_deref().unwrap().starts_with("Not found"));
        assert_eq!(entries[0].tool, "xtiger_reports");
        assert_eq!(entries[0].client, None);
        assert_eq!(entries[0].outcome, Some(Outcome::Error));
        assert!(entries[0].summary.as_deref().unwrap().contains("No results yet"));
        assert_eq!(entries[0].args["severity"], "error");
        assert_eq!(journal::read_running(&state).len(), 0);
    }

    #[test]
    fn calls_share_a_session_until_the_job_is_wrapped_up() {
        let tmp = TempDir::new();
        let shared: Sessions = Arc::default();
        let run = |name: &str, arguments: Value| {
            let call = Call {
                id: json!(1),
                params: json!({"name": name, "arguments": arguments}),
                out: Arc::new(Mutex::new(Box::new(Captured::default()))),
                client: None,
                sessions: Arc::clone(&shared),
            };
            let flags = (Arc::default(), Arc::default());
            start_call(call, detect_in(tmp.to_path_buf()), flags).join().unwrap();
        };
        run(sessions::TOOL, json!({"title": "Fix the events"}));
        run("xtiger_status", json!({}));
        run(sessions::TOOL, json!({"wrap_up": "Nothing  to fix."}));
        run("xtiger_status", json!({}));
        run("nope", json!({}));
        // Newest first.
        let entries = journal::read_entries(&tmp.join("state"), 10);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[3].summary.as_deref(), Some("Started: Fix the events"));
        assert!(entries[3].session.is_some());
        assert_eq!(entries[3].session, entries[2].session);
        assert_eq!(entries[2].session, entries[1].session);
        assert_eq!(entries[1].summary.as_deref(), Some("Nothing to fix."));
        assert_ne!(entries[0].session, entries[1].session);
    }

    #[test]
    fn images_are_sent_inline() {
        let tmp = TempDir::new();
        let png = tmp.join("shot.png");
        fs::write(&png, b"\x89PNG").unwrap();
        let content = content_json(vec![
            Content::Text("hi".into()),
            Content::Image(png),
            Content::Image(tmp.join("gone.png")),
        ]);
        assert_eq!(
            content[1],
            json!({"type": "image", "data": "iVBORw==", "mimeType": "image/png"})
        );
        assert!(content[2]["text"].as_str().unwrap().contains("could not be read"));
    }
}
