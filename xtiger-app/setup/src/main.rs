//! The xTiger setup: installs the app for the current user and removes it again.
//!
//! Without arguments it shows the installer. With `--uninstall` it shows the uninstaller, which
//! is how Windows starts the copy kept in the install folder. Add `--silent` to do either without
//! a window: `--dir <folder>` and `--desktop` choose where to install and whether to add a desktop
//! shortcut, and `--remove-data` also deletes the app's settings and history when uninstalling.
//! The app starts a downloaded setup with `--update`: it then installs over the existing copy
//! without asking, once the app has closed, and opens it again.

// Do not open a console window next to the setup in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(windows))]
compile_error!("The xTiger setup is for Windows only.");

mod install;

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::window::Color;
use tauri::{Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};

use crate::install::Options;

/// The name of this release, shown under the version.
const RELEASE_NAME: &str = "Frankokratia";

#[derive(Default)]
struct Args {
    uninstall: bool,
    update: bool,
    silent: bool,
    desktop: bool,
    remove_data: bool,
    dir: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut args = Args::default();
    let mut iter = std::env::args_os().skip(1);
    while let Some(arg) = iter.next() {
        match arg.to_str() {
            Some("--uninstall") => args.uninstall = true,
            Some("--update") => args.update = true,
            Some("--silent") => args.silent = true,
            Some("--desktop") => args.desktop = true,
            Some("--remove-data") => args.remove_data = true,
            Some("--dir") => args.dir = iter.next().map(PathBuf::from),
            _ => {}
        }
    }
    args
}

struct Session {
    uninstall: bool,
    update: bool,
    /// The folder that was uninstalled, if any.
    uninstalled: Mutex<Option<PathBuf>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    mode: &'static str,
    version: &'static str,
    release_name: &'static str,
    ready: bool,
    default_dir: Option<PathBuf>,
    existing: Option<ExistingInfo>,
}

#[derive(Serialize)]
struct ExistingInfo {
    dir: PathBuf,
    version: String,
    desktop: bool,
}

#[derive(Clone, Serialize)]
struct Progress {
    done: u64,
    total: u64,
    file: String,
}

#[tauri::command]
fn info(session: State<Session>) -> Info {
    let existing = install::existing();
    Info {
        mode: if session.uninstall {
            "uninstall"
        } else if session.update {
            "update"
        } else {
            "install"
        },
        version: install::VERSION,
        release_name: RELEASE_NAME,
        ready: install::has_payload(),
        default_dir: existing.as_ref().map(|e| e.dir.clone()).or_else(install::default_dir),
        existing: existing.map(|e| ExistingInfo { dir: e.dir, version: e.version, desktop: e.desktop }),
    }
}

#[tauri::command]
async fn install(dir: PathBuf, desktop: bool, on_progress: Channel<Progress>) -> Result<(), String> {
    run_blocking(move || {
        // Report each whole percent and each new file, not every block written.
        let mut last = (u64::MAX, String::new());
        install::install(&Options { dir, desktop }, |done, total, file| {
            let percent = done * 100 / total.max(1);
            if percent != last.0 || file != last.1 {
                last = (percent, file.to_owned());
                let _ = on_progress.send(Progress { done, total, file: file.to_owned() });
            }
        })
    })
    .await
}

#[tauri::command]
async fn uninstall(remove_data: bool, session: State<'_, Session>) -> Result<(), String> {
    let dir = install::installed_dir().ok_or("xTiger is not installed.")?;
    let removed = dir.clone();
    run_blocking(move || install::uninstall(&dir, remove_data)).await?;
    *session.uninstalled.lock().unwrap() = Some(removed);
    Ok(())
}

/// Wait until the app that started this update has closed, for at most half a minute.
#[tauri::command]
async fn wait_for_app(dir: PathBuf) -> Result<(), String> {
    run_blocking(move || {
        for _ in 0..60 {
            if install::check_not_running(&dir).is_ok() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        install::check_not_running(&dir)
    })
    .await
}

#[tauri::command]
fn launch(dir: PathBuf) -> Result<(), String> {
    install::launch(&dir)
}

async fn run_blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(job).await.map_err(|e| e.to_string())?
}

/// Where the setup's own WebView2 keeps its data. It is deleted again when the setup closes.
fn webview_data() -> PathBuf {
    std::env::temp_dir().join("xtiger-setup")
}

fn main() {
    let args = parse_args();
    if args.silent {
        let result = silent(&args);
        if let Err(message) = &result {
            eprintln!("{message}");
        }
        std::process::exit(i32::from(result.is_err()));
    }
    if tauri::webview_version().is_err() {
        missing_webview();
        return;
    }

    let uninstall_mode = args.uninstall;
    let update_mode = args.update && !args.uninstall;
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Session { uninstall: uninstall_mode, update: update_mode, uninstalled: Mutex::new(None) })
        .invoke_handler(tauri::generate_handler![info, install, uninstall, wait_for_app, launch])
        .setup(move |app| {
            let title = if uninstall_mode {
                "Uninstall xTiger"
            } else if update_mode {
                "Updating xTiger"
            } else {
                "xTiger Setup"
            };
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("setup/index.html".into()))
                .title(title)
                .inner_size(760.0, 480.0)
                .resizable(false)
                .maximizable(false)
                .decorations(false)
                .center()
                .background_color(Color(10, 21, 48, 255))
                .data_directory(webview_data())
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("cannot start the xTiger setup")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                let uninstalled = app.state::<Session>().uninstalled.lock().unwrap().take();
                install::clean_up_after_exit(uninstalled.as_deref(), &webview_data());
            }
        });
}

fn silent(args: &Args) -> Result<(), String> {
    if args.uninstall {
        let dir = install::installed_dir().ok_or("xTiger is not installed.")?;
        install::uninstall(&dir, args.remove_data)?;
        install::clean_up_after_exit(Some(&dir), &webview_data());
        return Ok(());
    }
    let dir = args
        .dir
        .clone()
        .or_else(|| install::existing().map(|e| e.dir))
        .or_else(install::default_dir)
        .ok_or("Cannot tell where to install xTiger. Pass --dir <folder>.")?;
    install::install(&Options { dir, desktop: args.desktop }, |_, _, _| {})
}

/// Shown instead of the setup when Windows lacks Microsoft Edge WebView2, which both the setup
/// and the app need. Windows 11 always has it.
fn missing_webview() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IDYES, MB_ICONINFORMATION, MB_YESNO, MessageBoxW};

    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let text = wide(
        "xTiger needs Microsoft Edge WebView2, which this copy of Windows doesn't have yet.\n\n\
         Open the download page? Run the setup again once WebView2 is installed.",
    );
    let title = wide("xTiger Setup");
    // SAFETY: both strings are NUL-terminated and live until the call returns.
    let answer = unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_YESNO | MB_ICONINFORMATION) };
    if answer == IDYES {
        let _ = std::process::Command::new("explorer.exe")
            .arg("https://developer.microsoft.com/microsoft-edge/webview2/")
            .spawn();
    }
}
