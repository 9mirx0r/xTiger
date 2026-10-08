//! Opening a report's file at its line.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// VS Code's command line launcher, from the PATH or the usual install folders.
fn find_vscode() -> Option<PathBuf> {
    let name = if cfg!(windows) { "code.cmd" } else { "code" };
    let mut dirs: Vec<PathBuf> =
        env::var_os("PATH").map(|path| env::split_paths(&path).collect()).unwrap_or_default();
    for var in ["LOCALAPPDATA", "ProgramFiles"] {
        if let Some(base) = env::var_os(var) {
            let base = PathBuf::from(base);
            dirs.push(base.join("Programs/Microsoft VS Code/bin"));
            dirs.push(base.join("Microsoft VS Code/bin"));
        }
    }
    dirs.into_iter().map(|dir| dir.join(name)).find(|path| path.is_file())
}

fn no_window(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

pub fn has_vscode() -> bool {
    find_vscode().is_some()
}

/// Open the file at the line in VS Code, or with the file's default program.
pub fn open(
    path: &Path,
    line: Option<u32>,
    column: Option<u32>,
    prefer_vscode: bool,
) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!("{} does not exist", path.display()));
    }
    if prefer_vscode && let Some(code) = find_vscode() {
        let target = format!("{}:{}:{}", path.display(), line.unwrap_or(1), column.unwrap_or(1));
        return no_window(Command::new(code).arg("-g").arg(target))
            .spawn()
            .map(drop)
            .map_err(|e| e.to_string());
    }
    open_default(path)
}

#[cfg(windows)]
fn open_default(path: &Path) -> Result<(), String> {
    Command::new("explorer").arg(path).spawn().map(drop).map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn open_default(path: &Path) -> Result<(), String> {
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    Command::new(opener).arg(path).spawn().map(drop).map_err(|e| e.to_string())
}

/// Show the file or folder in the system's file manager.
#[cfg(windows)]
pub fn reveal(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    Command::new("explorer")
        .raw_arg(format!("/select,\"{}\"", path.display()))
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
pub fn reveal(path: &Path) -> Result<(), String> {
    open_default(path.parent().unwrap_or(path))
}
