use ck3_tiger::GAME_CONSTS;
use tiger_bin_shared::{before_closing, tiger};

fn main() {
    // Exit status 1 means the mod has problems at or above --fail-on. Anything that stops the run
    // before that (bad arguments, no game directory, invalid config) is status 2.
    let result = tiger(GAME_CONSTS, env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_NAME"));
    if let Err(e) = &result {
        eprintln!("Error: {e:?}");
    }
    // A double-click opens a window that closes at once; keep it until the result has been read.
    before_closing();
    if result.is_err() {
        std::process::exit(2);
    }
}
