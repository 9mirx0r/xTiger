//! Keeps the window open when the program was started with a double-click.

use std::sync::atomic::{AtomicBool, Ordering};

static FINISHED: AtomicBool = AtomicBool::new(false);

/// The user chose to finish on their own, so there is nothing left to read in the window.
pub fn finished() {
    FINISHED.store(true, Ordering::Relaxed);
}

/// Wait for Enter when the program owns its console window alone, which is what a double-click in
/// the file explorer does. Started from a terminal, the window belongs to the shell and stays, so
/// nothing is waited for. Without this, the window closes the moment the program ends, with the
/// result or the error still unread.
pub fn before_closing() {
    if FINISHED.load(Ordering::Relaxed) || !opened_by_double_click() {
        return;
    }
    eprintln!("\nPress Enter to close this window.");
    let _ = std::io::stdin().read_line(&mut String::new());
}

#[cfg(windows)]
fn opened_by_double_click() -> bool {
    unsafe extern "system" {
        fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
    }
    let mut list = [0_u32; 2];
    // SAFETY: the list is a writable array of the length that is passed along.
    let attached = unsafe { GetConsoleProcessList(list.as_mut_ptr(), 2) };
    attached == 1
}

#[cfg(not(windows))]
fn opened_by_double_click() -> bool {
    false
}
