//! Embeds the real installer (the NSIS setup built from the main app) into this
//! executable. Set `QUAY_SETUP_PAYLOAD` to its path; without it the payload is empty
//! and the setup app reports that instead of installing, which keeps dev and CI builds
//! working.

use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=QUAY_SETUP_PAYLOAD");
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("payload.exe");
    match env::var("QUAY_SETUP_PAYLOAD") {
        Ok(path) if !path.is_empty() => {
            println!("cargo:rerun-if-changed={path}");
            fs::copy(&path, &out)
                .unwrap_or_else(|e| panic!("cannot read QUAY_SETUP_PAYLOAD {path}: {e}"));
        }
        _ => fs::write(&out, b"").expect("write empty payload"),
    }
    tauri_build::build();
}
