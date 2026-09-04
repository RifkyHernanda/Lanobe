//! The WebUI is embedded with rust-embed, which fails the build if its folder is
//! missing. `make webui` populates it, but a bare `cargo build` (CI check, clippy,
//! a fresh clone) should still work - so make sure the directory exists, with a
//! placeholder page explaining what is missing.
use std::{fs, path::Path};

const PLACEHOLDER: &str = r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Lanobe</title></head>
<body><p>WebUI assets were not built into this binary. Run <code>make webui</code> and rebuild.</p></body>
</html>
"#;

fn main() {
    let webui = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/webui");

    if let Err(err) = fs::create_dir_all(&webui) {
        panic!("failed to create {}: {err}", webui.display());
    }

    let index = webui.join("index.html");
    if !index.exists()
        && let Err(err) = fs::write(&index, PLACEHOLDER)
    {
        panic!("failed to write {}: {err}", index.display());
    }

    println!("cargo:rerun-if-changed=resources/webui");
}
