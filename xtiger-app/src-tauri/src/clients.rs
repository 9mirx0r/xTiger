//! Connecting AI assistants to xTiger: adding the `xtiger` MCP server to their settings, or
//! taking it out again.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value, json};

/// The name the server gets in every assistant's settings.
pub const SERVER_NAME: &str = "xtiger";

pub const SERVER_EXE: &str = if cfg!(windows) { "xtiger-mcp.exe" } else { "xtiger-mcp" };

/// How an assistant lists its MCP servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    /// `"mcpServers": {"name": {"command": ...}}`, used by most.
    McpServers,
    /// Like `McpServers`, with `"type": "stdio"` in each entry.
    McpServersTyped,
    /// VS Code: `"servers": {"name": {"type": "stdio", "command": ...}}`.
    Servers,
}

impl Style {
    fn key(self) -> &'static str {
        match self {
            Style::McpServers | Style::McpServersTyped => "mcpServers",
            Style::Servers => "servers",
        }
    }

    fn entry(self, server: &Path) -> Value {
        let command = server.display().to_string();
        match self {
            Style::McpServers => json!({"command": command, "args": []}),
            Style::McpServersTyped | Style::Servers => {
                json!({"type": "stdio", "command": command, "args": []})
            }
        }
    }
}

struct Client {
    id: &'static str,
    name: &'static str,
    style: Style,
    /// The settings files to change. The client counts as installed when the folder of one of
    /// them exists.
    files: Vec<PathBuf>,
    /// What the user has to do afterwards for the change to take effect.
    after: &'static str,
}

fn config_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|home| home.join("Library/Application Support"))
    } else {
        home().map(|home| home.join(".config"))
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// Claude Desktop from the Microsoft Store keeps its settings inside its package folder.
fn claude_store_dirs() -> Vec<PathBuf> {
    let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(local.join("Packages")) else { return Vec::new() };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("Claude_"))
        .map(|entry| entry.path().join("LocalCache/Roaming/Claude"))
        .filter(|dir| dir.is_dir())
        .collect()
}

fn clients() -> Vec<Client> {
    let home = home().unwrap_or_default();
    let config = config_dir().unwrap_or_default();
    let mut claude_dirs = vec![config.join("Claude")];
    claude_dirs.extend(claude_store_dirs());
    vec![
        Client {
            id: "claude-desktop",
            name: "Claude Desktop",
            style: Style::McpServers,
            files: claude_dirs
                .into_iter()
                .map(|dir| dir.join("claude_desktop_config.json"))
                .collect(),
            after: "Quit Claude Desktop completely (also from the tray) and open it again.",
        },
        Client {
            id: "claude-code",
            name: "Claude Code",
            style: Style::McpServersTyped,
            files: vec![home.join(".claude.json")],
            after: "Start a new Claude Code session.",
        },
        Client {
            id: "cursor",
            name: "Cursor",
            style: Style::McpServers,
            files: vec![home.join(".cursor/mcp.json")],
            after: "Cursor picks it up by itself; check Settings > MCP.",
        },
        Client {
            id: "vscode",
            name: "VS Code",
            style: Style::Servers,
            files: vec![config.join("Code/User/mcp.json")],
            after: "In VS Code, run \"MCP: List Servers\" and start xtiger, or reload the window.",
        },
        Client {
            id: "windsurf",
            name: "Windsurf",
            style: Style::McpServers,
            files: vec![home.join(".codeium/windsurf/mcp_config.json")],
            after: "Press refresh in Windsurf's MCP panel.",
        },
    ]
}

/// Claude Code keeps `.claude.json` in the home folder itself, so its folder always exists;
/// look for the `.claude` folder instead.
fn installed(client: &Client) -> bool {
    if client.id == "claude-code" {
        return home().is_some_and(|home| {
            home.join(".claude").is_dir() || home.join(".claude.json").is_file()
        });
    }
    client.files.iter().any(|file| file.parent().is_some_and(Path::is_dir))
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum State {
    /// The assistant was not found on this computer.
    Missing,
    /// Installed, without xTiger.
    Available,
    /// xTiger is in its settings and points to this copy.
    Connected,
    /// xTiger is in its settings but points to another program, for example an older copy.
    Elsewhere,
    /// Its settings file cannot be read as plain JSON, for example because it has comments.
    Unreadable,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientStatus {
    id: &'static str,
    name: &'static str,
    state: State,
    /// The settings file that is or would be changed.
    file: Option<PathBuf>,
    /// The program the existing entry runs, when it is not this copy.
    other_command: Option<String>,
    /// What to do after connecting.
    after: &'static str,
    /// The text to paste into the settings by hand, when they cannot be changed safely.
    snippet: String,
}

fn read_json(path: &Path) -> Result<Option<Map<String, Value>>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let text = text.trim_start_matches('\u{feff}');
    if text.trim().is_empty() {
        return Ok(Some(Map::new()));
    }
    match serde_json::from_str(text) {
        Ok(Value::Object(map)) => Ok(Some(map)),
        Ok(_) => Err(format!("{} does not hold a JSON object.", path.display())),
        Err(e) => Err(format!(
            "{} is not plain JSON ({e}). It may have comments; add xTiger by hand with the text below.",
            path.display()
        )),
    }
}

/// Replace the file in one step, so a crash never leaves half a file. The old file is kept
/// next to it as `.xtiger-backup`.
fn write_json(path: &Path, map: &Map<String, Value>) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let mut text = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    text.push('\n');
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    if path.is_file() {
        let backup = path.with_file_name(format!("{name}.xtiger-backup"));
        fs::copy(path, &backup).map_err(|e| format!("cannot back up {}: {e}", path.display()))?;
    }
    let temp = path.with_file_name(format!("{name}.xtiger-new"));
    fs::write(&temp, text).map_err(|e| format!("cannot write {}: {e}", temp.display()))?;
    fs::rename(&temp, path).map_err(|e| {
        let _ = fs::remove_file(&temp);
        format!("cannot replace {}: {e}", path.display())
    })
}

fn same_program(command: &str, server: &Path) -> bool {
    let command = Path::new(command);
    match (command.canonicalize(), server.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => command == server,
    }
}

fn snippet(style: Style, server: &Path) -> String {
    let doc = json!({ style.key(): { SERVER_NAME: style.entry(server) } });
    serde_json::to_string_pretty(&doc).unwrap_or_default()
}

/// What one settings file says about xTiger.
fn file_state(path: &Path, style: Style, server: &Path) -> (State, Option<String>) {
    match read_json(path) {
        Err(_) => (State::Unreadable, None),
        Ok(None) => (State::Available, None),
        Ok(Some(map)) => match map.get(style.key()).and_then(|servers| servers.get(SERVER_NAME)) {
            None => (State::Available, None),
            Some(entry) => {
                let command = entry.get("command").and_then(Value::as_str).unwrap_or("");
                if same_program(command, server) {
                    (State::Connected, None)
                } else {
                    (State::Elsewhere, Some(command.to_owned()))
                }
            }
        },
    }
}

fn status_of(client: &Client, server: &Path) -> ClientStatus {
    let snippet = snippet(client.style, server);
    let file = client.files.first().cloned();
    let mut status = ClientStatus {
        id: client.id,
        name: client.name,
        state: State::Missing,
        file,
        other_command: None,
        after: client.after,
        snippet,
    };
    if !installed(client) {
        return status;
    }
    // With several files (Claude Desktop installed twice), the least connected one counts.
    let mut worst: Option<(State, Option<String>, PathBuf)> = None;
    for file in
        client.files.iter().filter(|file| file.parent().is_some_and(Path::is_dir) || file.is_file())
    {
        let (state, other) = file_state(file, client.style, server);
        let rank = |state: &State| match state {
            State::Unreadable => 0,
            State::Available => 1,
            State::Elsewhere => 2,
            State::Connected => 3,
            State::Missing => 4,
        };
        if worst.as_ref().is_none_or(|(old, ..)| rank(&state) < rank(old)) {
            worst = Some((state, other, file.clone()));
        }
    }
    let (state, other, file) =
        worst.unwrap_or((State::Available, None, status.file.clone().unwrap_or_default()));
    status.state = state;
    status.other_command = other;
    status.file = Some(file);
    status
}

/// Where this copy's MCP server is: next to the app, as the setup and the archives install it.
pub fn server_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let server = exe.parent()?.join(SERVER_EXE);
    server.is_file().then_some(server)
}

/// The setup cannot replace programs an assistant is running, so it renames them to `*.old`.
/// Remove those once the assistants have let go of them.
pub fn remove_old_copies() {
    let Some(dir) =
        std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf))
    else {
        return;
    };
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().ends_with(".old") {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    /// This copy's MCP server, if it is there.
    server: Option<PathBuf>,
    clients: Vec<ClientStatus>,
    /// The settings text for any other assistant.
    snippet: String,
}

pub fn overview() -> Overview {
    let server = server_path();
    let shown = server.clone().unwrap_or_else(|| PathBuf::from(SERVER_EXE));
    Overview {
        clients: clients().iter().map(|client| status_of(client, &shown)).collect(),
        snippet: snippet(Style::McpServers, &shown),
        server,
    }
}

fn find(id: &str) -> Result<Client, String> {
    clients()
        .into_iter()
        .find(|client| client.id == id)
        .ok_or_else(|| format!("Unknown assistant {id}"))
}

fn files_to_change(client: &Client) -> Vec<&PathBuf> {
    client
        .files
        .iter()
        .filter(|file| file.parent().is_some_and(Path::is_dir) || file.is_file())
        .collect()
}

/// Add xTiger to an assistant's settings, or point an older entry to this copy.
pub fn connect(id: &str) -> Result<ClientStatus, String> {
    let server = server_path().ok_or_else(|| {
        format!("{SERVER_EXE} is missing next to xTiger. Reinstall xTiger to get it back.")
    })?;
    let client = find(id)?;
    if !installed(&client) {
        return Err(format!("{} was not found on this computer.", client.name));
    }
    for file in files_to_change(&client) {
        set_entry(file, client.style, Some(&server))?;
    }
    Ok(status_of(&client, &server))
}

/// Take xTiger out of an assistant's settings.
pub fn disconnect(id: &str) -> Result<ClientStatus, String> {
    let client = find(id)?;
    for file in files_to_change(&client) {
        set_entry(file, client.style, None)?;
    }
    let shown = server_path().unwrap_or_else(|| PathBuf::from(SERVER_EXE));
    Ok(status_of(&client, &shown))
}

/// Put the entry in (`Some`) or take it out (`None`). Everything else in the file is kept as it
/// was, in the same order.
fn set_entry(file: &Path, style: Style, server: Option<&Path>) -> Result<(), String> {
    let mut map = read_json(file)?.unwrap_or_default();
    let servers = map.entry(style.key()).or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(servers) = servers else {
        return Err(format!("\"{}\" in {} is not an object.", style.key(), file.display()));
    };
    match server {
        Some(server) => {
            // Keep settings the user added to an existing entry, such as environment variables.
            let mut entry = match servers.remove(SERVER_NAME) {
                Some(Value::Object(old)) => old,
                _ => Map::new(),
            };
            if let Value::Object(fresh) = style.entry(server) {
                for (key, value) in fresh {
                    entry.insert(key, value);
                }
            }
            servers.insert(SERVER_NAME.to_owned(), Value::Object(entry));
        }
        None => {
            if servers.remove(SERVER_NAME).is_none() {
                return Ok(());
            }
        }
    }
    write_json(file, &map)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Temp(PathBuf);

    impl Temp {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("xtiger-clients-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn adds_and_removes_the_entry_keeping_the_rest() {
        let tmp = Temp::new("keep");
        let file = tmp.0.join("config.json");
        let server = tmp.0.join("xtiger-mcp.exe");
        fs::write(&server, "").unwrap();
        fs::write(
            &file,
            r#"{"zeta": 1, "mcpServers": {"other": {"command": "x"}, "xtiger": {"command": "old", "env": {"A": "1"}}}, "alpha": 2}"#,
        )
        .unwrap();
        assert_eq!(
            file_state(&file, Style::McpServers, &server),
            (State::Elsewhere, Some("old".to_owned()))
        );
        set_entry(&file, Style::McpServers, Some(&server)).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.find("zeta").unwrap() < text.find("alpha").unwrap(), "order kept: {text}");
        let map: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(map["mcpServers"]["xtiger"]["env"]["A"], "1");
        assert_eq!(map["mcpServers"]["other"]["command"], "x");
        assert_eq!(file_state(&file, Style::McpServers, &server), (State::Connected, None));
        assert!(tmp.0.join("config.json.xtiger-backup").is_file());
        set_entry(&file, Style::McpServers, None).unwrap();
        let map: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert!(map["mcpServers"].get("xtiger").is_none());
        assert_eq!(map["mcpServers"]["other"]["command"], "x");
    }

    #[test]
    fn creates_missing_files_and_refuses_comments() {
        let tmp = Temp::new("new");
        let file = tmp.0.join("sub/mcp.json");
        let server = tmp.0.join("xtiger-mcp.exe");
        assert_eq!(file_state(&file, Style::Servers, &server), (State::Available, None));
        set_entry(&file, Style::Servers, Some(&server)).unwrap();
        let map: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(map["servers"]["xtiger"]["type"], "stdio");
        fs::write(&file, "// comment\n{}").unwrap();
        assert_eq!(file_state(&file, Style::Servers, &server).0, State::Unreadable);
        assert!(
            set_entry(&file, Style::Servers, Some(&server)).unwrap_err().contains("not plain JSON")
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), "// comment\n{}");
    }

    #[test]
    fn snippets_are_ready_to_paste() {
        let text = snippet(Style::McpServers, Path::new("C:/xTiger/xtiger-mcp.exe"));
        let map: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(map["mcpServers"]["xtiger"]["command"], "C:/xTiger/xtiger-mcp.exe");
    }
}
