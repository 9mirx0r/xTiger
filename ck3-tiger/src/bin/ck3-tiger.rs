use ck3_tiger::GAME_CONSTS;
use tiger_bin_shared::tiger;

fn main() {
    // Exit status 1 means the mod has problems at or above --fail-on. Anything that stops the run
    // before that (bad arguments, no game directory, invalid config) is status 2.
    if let Err(e) = tiger(GAME_CONSTS, env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_NAME")) {
        eprintln!("Error: {e:?}");
        std::process::exit(2);
    }
}
