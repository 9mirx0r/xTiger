//! Embeds the files to install. `build-release.ps1` packs them into a zip and passes its path in
//! `XTIGER_SETUP_PAYLOAD`. Without it the setup builds with an empty payload, which is enough to
//! work on its screens, but it refuses to install.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=XTIGER_SETUP_PAYLOAD");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("payload.zip");
    match std::env::var_os("XTIGER_SETUP_PAYLOAD") {
        Some(payload) => {
            println!("cargo:rerun-if-changed={}", PathBuf::from(&payload).display());
            std::fs::copy(&payload, &out).expect("cannot read XTIGER_SETUP_PAYLOAD");
        }
        None => std::fs::write(&out, []).unwrap(),
    }
    tauri_build::build();
}
