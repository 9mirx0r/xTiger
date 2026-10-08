//! Putting xTiger on the computer and taking it off again. Everything is per user, so nothing
//! here needs administrator rights.

use std::fs::{self, OpenOptions};
use std::io::{self, Cursor, Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_EXE: &str = "xTiger.exe";
const VALIDATOR_EXE: &str = "ck3-tiger.exe";
/// The MCP server, which AI assistants keep running while they are open.
const MCP_EXE: &str = "xtiger-mcp.exe";
pub const UNINSTALLER: &str = "uninstall.exe";
const SHORTCUT: &str = "xTiger.lnk";
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\xTiger";
/// The app's identifier, which names the folders with its settings and history.
const APP_ID: &str = "dev.xtiger.desktop";

/// The files to install, packed by `build.rs`.
static PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.zip"));

pub fn has_payload() -> bool {
    !PAYLOAD.is_empty()
}

pub fn default_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|dir| dir.join("Programs").join("xTiger"))
}

/// What an earlier install left in the registry.
pub struct Existing {
    pub dir: PathBuf,
    pub version: String,
    pub desktop: bool,
}

pub fn existing() -> Option<Existing> {
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey(UNINSTALL_KEY).ok()?;
    let dir: String = key.get_value("InstallLocation").ok()?;
    Some(Existing {
        dir: dir.into(),
        version: key.get_value("DisplayVersion").unwrap_or_default(),
        desktop: key.get_value::<u32, _>("DesktopShortcut").is_ok_and(|v| v != 0),
    })
}

pub struct Options {
    pub dir: PathBuf,
    pub desktop: bool,
}

/// Installs into `opts.dir`, calling `progress(done, total, file)` as the bytes are written.
pub fn install(opts: &Options, mut progress: impl FnMut(u64, u64, &str)) -> Result<(), String> {
    if !has_payload() {
        return Err("This copy of the setup was built without xTiger inside, so there is nothing to install.".into());
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(PAYLOAD)).map_err(|e| format!("The setup is damaged: {e}"))?;
    let total: u64 = (0..zip.len()).filter_map(|i| zip.by_index_raw(i).ok().map(|f| f.size())).sum();

    fs::create_dir_all(&opts.dir).map_err(|e| failed("create", &opts.dir, &e))?;
    check_not_running(&opts.dir)?;
    remove_old(&opts.dir);

    let mut done = 0;
    let mut buffer = vec![0; 1 << 16];
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("The setup is damaged: {e}"))?;
        let Some(name) = entry.enclosed_name() else { continue };
        let path = opts.dir.join(&name);
        if entry.is_dir() {
            fs::create_dir_all(&path).map_err(|e| failed("create", &path, &e))?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| failed("create", parent, &e))?;
        }
        let name = name.to_string_lossy().replace('\\', "/");
        progress(done, total, &name);
        make_room(&path)?;
        let mut file = fs::File::create(&path).map_err(|e| failed("write", &path, &e))?;
        loop {
            let read = entry.read(&mut buffer).map_err(|e| format!("The setup is damaged: {e}"))?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read]).map_err(|e| failed("write", &path, &e))?;
            done += read as u64;
            progress(done, total, &name);
        }
    }

    // Keep a copy of this program as the uninstaller, unless that is what is running.
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let uninstaller = opts.dir.join(UNINSTALLER);
    if !same_file(&me, &uninstaller) {
        fs::copy(&me, &uninstaller).map_err(|e| failed("write", &uninstaller, &e))?;
    }

    let app = opts.dir.join(APP_EXE);
    if let Some(dir) = start_menu_dir() {
        shortcut(&app, &opts.dir, &dir.join(SHORTCUT))?;
    }
    if opts.desktop
        && let Some(dir) = dirs::desktop_dir() {
            shortcut(&app, &opts.dir, &dir.join(SHORTCUT))?;
        }

    let uninstaller_size = fs::metadata(&uninstaller).map_or(0, |m| m.len());
    register(opts, total + uninstaller_size).map_err(|e| format!("Cannot register xTiger with Windows: {e}"))
}

/// Removes what [`install`] put in `dir`, its shortcuts and its registry entry. The uninstaller
/// itself is still running, so [`clean_up_after_exit`] removes it later.
pub fn uninstall(dir: &Path, remove_data: bool) -> Result<(), String> {
    check_not_running(dir)?;
    for name in [APP_EXE, VALIDATOR_EXE, MCP_EXE] {
        let path = dir.join(name);
        make_room(&path)?;
        remove(path, fs::remove_file)?;
    }
    remove_old(dir);
    remove(dir.join("licenses"), fs::remove_dir_all)?;
    // Old portable copies may have left this behind.
    remove(dir.join("portable.txt"), fs::remove_file)?;

    for folder in [start_menu_dir(), dirs::desktop_dir()].into_iter().flatten() {
        remove(folder.join(SHORTCUT), fs::remove_file)?;
    }
    match RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(UNINSTALL_KEY) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(format!("Cannot unregister xTiger: {e}")),
        _ => {}
    }

    if remove_data {
        for base in [dirs::data_dir(), dirs::data_local_dir()].into_iter().flatten() {
            remove(base.join(APP_ID), fs::remove_dir_all)?;
        }
    }
    Ok(())
}

/// The folder to uninstall from: the one in the registry, or else the one this uninstaller is in.
pub fn installed_dir() -> Option<PathBuf> {
    if let Some(existing) = existing() {
        return Some(existing.dir);
    }
    let me = std::env::current_exe().ok()?;
    let is_uninstaller = me.file_name().is_some_and(|name| name.eq_ignore_ascii_case(UNINSTALLER));
    if is_uninstaller { me.parent().map(Path::to_path_buf) } else { None }
}

pub fn launch(dir: &Path) -> Result<(), String> {
    Command::new(dir.join(APP_EXE))
        .current_dir(dir)
        .spawn()
        .map(drop)
        .map_err(|e| format!("Cannot start xTiger: {e}"))
}

/// A running program can't delete itself or the WebView2 data it is using. This starts a hidden
/// command that waits for this process to end and then removes `webview_data` and, after an
/// uninstall, the uninstaller and the install folder if nothing else is left in it.
pub fn clean_up_after_exit(uninstalled: Option<&Path>, webview_data: &Path) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut script = String::from("/d /c ping -n 3 127.0.0.1 >nul");
    script += &format!(r#" & rmdir /s /q "{}""#, webview_data.display());
    if let Some(dir) = uninstalled {
        script += &format!(
            r#" & del /f /q "{}" "{}" & rmdir "{}""#,
            dir.join(UNINSTALLER).display(),
            dir.join("*.old").display(),
            dir.display()
        );
    }
    let _ = Command::new("cmd.exe")
        .raw_arg(script)
        .current_dir(std::env::temp_dir())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

fn register(opts: &Options, size: u64) -> io::Result<()> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(UNINSTALL_KEY)?;
    let dir = opts.dir.display().to_string();
    let uninstaller = opts.dir.join(UNINSTALLER).display().to_string();
    key.set_value("DisplayName", &"xTiger")?;
    key.set_value("DisplayVersion", &VERSION)?;
    key.set_value("Publisher", &"Qubis")?;
    key.set_value("DisplayIcon", &opts.dir.join(APP_EXE).display().to_string())?;
    key.set_value("InstallLocation", &dir)?;
    key.set_value("UninstallString", &format!(r#""{uninstaller}" --uninstall"#))?;
    key.set_value("QuietUninstallString", &format!(r#""{uninstaller}" --uninstall --silent"#))?;
    key.set_value("URLInfoAbout", &"https://github.com/9mirx0r/xTiger")?;
    key.set_value("HelpLink", &"https://github.com/9mirx0r/xTiger/issues")?;
    key.set_value("EstimatedSize", &u32::try_from(size / 1024).unwrap_or(u32::MAX))?;
    key.set_value("NoModify", &1u32)?;
    key.set_value("NoRepair", &1u32)?;
    key.set_value("DesktopShortcut", &u32::from(opts.desktop))?;
    Ok(())
}

fn shortcut(target: &Path, dir: &Path, path: &Path) -> Result<(), String> {
    let mut link = mslnk::ShellLink::new(target).map_err(|e| format!("Cannot create a shortcut: {e}"))?;
    link.set_working_dir(Some(dir.display().to_string()));
    link.set_name(Some("Check Crusader Kings III mods for mistakes".into()));
    link.create_lnk(path).map_err(|e| format!("Cannot create {}: {e}", path.display()))
}

fn start_menu_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join(r"Microsoft\Windows\Start Menu\Programs"))
}

/// Windows refuses to open a running program for writing.
fn in_use(path: &Path) -> bool {
    const SHARING_VIOLATION: i32 = 32;
    OpenOptions::new().write(true).open(path).is_err_and(|e| e.raw_os_error() == Some(SHARING_VIOLATION))
}

/// Tells whether xTiger itself is still open. The validator and the MCP server may also be running
/// for an AI assistant, which the user should not have to close: [`make_room`] handles those.
pub fn check_not_running(dir: &Path) -> Result<(), String> {
    if in_use(&dir.join(APP_EXE)) {
        return Err("xTiger is still open. Close it and try again.".into());
    }
    Ok(())
}

/// A running program cannot be replaced or deleted, but it can be renamed. This moves `path` aside
/// as `<name>.<n>.old` when it is in use, so that a new copy can take its place. The old copies
/// are removed by [`remove_old`] once nothing runs them any more.
fn make_room(path: &Path) -> Result<(), String> {
    if !in_use(path) {
        return Ok(());
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    for n in 0..100 {
        let aside = path.with_file_name(format!("{name}.{n}.old"));
        if aside.exists() && fs::remove_file(&aside).is_err() {
            continue;
        }
        return fs::rename(path, &aside).map_err(|e| failed("replace", path, &e));
    }
    Err(format!("Cannot replace {}: it is in use. Close your AI assistants and try again.", path.display()))
}

/// Removes the copies that [`make_room`] moved aside, unless they are still running.
fn remove_old(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().ends_with(".old") {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn remove(path: PathBuf, how: fn(PathBuf) -> io::Result<()>) -> Result<(), String> {
    match how(path.clone()) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(failed("remove", &path, &e)),
        _ => Ok(()),
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn failed(what: &str, path: &Path, error: &io::Error) -> String {
    format!("Cannot {what} {}: {error}", path.display())
}
