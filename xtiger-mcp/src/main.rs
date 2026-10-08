//! `xtiger-mcp`: the MCP server, spoken over stdin and stdout.
//!
//! `xtiger-mcp --status` prints what it finds (the validator, the game and the folders) and exits.

use std::io::{self, BufReader};
use std::process::ExitCode;
use std::sync::Arc;

use xtiger_mcp::locate::Locations;
use xtiger_mcp::server;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => {
            let detect: server::Detect = Arc::new(Locations::detect);
            server::serve(BufReader::new(io::stdin()), Box::new(io::stdout()), &detect);
            ExitCode::SUCCESS
        }
        Some("--version" | "-V") => {
            println!("xtiger-mcp {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--status") => {
            let status = Locations::detect().describe();
            println!("{}", serde_json::to_string_pretty(&status).unwrap_or_default());
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") => {
            println!(
                "xtiger-mcp {}: MCP server for checking Crusader Kings III mods with xTiger.\n\n\
                 Run it from an AI assistant's MCP settings; it talks over stdin and stdout.\n\n\
                 Options:\n  --status   show which validator, game and folders it finds, and exit\n  \
                 --version  show the version and exit\n\n\
                 Everything is found automatically. To override: XTIGER_BIN (ck3-tiger), CK3_GAME_DIR, \
                 CK3_USER_DIR, XTIGER_STATE_DIR.",
                env!("CARGO_PKG_VERSION")
            );
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("xtiger-mcp: unknown option {other}. Try --help.");
            ExitCode::from(2)
        }
    }
}
