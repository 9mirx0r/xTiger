use ck3_tiger::GAME_CONSTS;
use tiger_bin_shared::{auto, before_closing};

fn main() {
    let result = auto(GAME_CONSTS);
    if let Err(e) = &result {
        eprintln!("Error: {e:?}");
    }
    // A double-click opens a window that closes at once; keep it until the result has been read.
    before_closing();
    if result.is_err() {
        std::process::exit(1);
    }
}
