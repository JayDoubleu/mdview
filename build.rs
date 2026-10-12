//! Embeds every file under `assets/` into the binary as a static lookup table.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn main() {
    // The .exe icon on Windows. Building on Windows it must work; when
    // cross-checking from another OS there may be no resource compiler, and
    // the build goes on without the icon.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=icons/mdview.rc");
        println!("cargo:rerun-if-changed=icons/mdview.ico");
        let result = embed_resource::compile("icons/mdview.rc", embed_resource::NONE);
        if cfg!(windows) {
            result.manifest_required().expect("embedding the app icon");
        }
    }

    let root = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("assets");
    println!("cargo:rerun-if-changed=assets");

    let mut files = Vec::new();
    collect(&root, &mut files);

    let mut src = String::from("pub static ASSETS: &[(&str, &[u8])] = &[\n");
    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
        let rel = f
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let abs = f.to_string_lossy().replace('\\', "/");
        writeln!(src, "    ({rel:?}, include_bytes!({abs:?})),").unwrap();
    }
    src.push_str("];\n");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("assets.rs");
    fs::write(out, src).unwrap();
}
