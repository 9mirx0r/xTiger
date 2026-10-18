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
        fn GetConsoleWindow() -> *mut core::ffi::c_void;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsWindowVisible(window: *mut core::ffi::c_void) -> i32;
    }
    let mut list = [0_u32; 2];
    // SAFETY: the list is a writable array of the length that is passed along.
    let attached = unsafe { GetConsoleProcessList(list.as_mut_ptr(), 2) };
    // A program started without a window (the desktop app starts the validator that way) also has
    // its console to itself, but there is no window to close and nobody to press Enter.
    // SAFETY: both calls only read the state of the console and of its window.
    let visible = unsafe {
        let window = GetConsoleWindow();
        !window.is_null() && IsWindowVisible(window) != 0
    };
    attached == 1 && visible
}

#[cfg(not(windows))]
fn opened_by_double_click() -> bool {
    false
}
