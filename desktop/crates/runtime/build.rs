use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn collect(root: &Path, directory: &Path, paths: &mut Vec<String>) {
    println!("cargo:rerun-if-changed={}", directory.display());
    for entry in fs::read_dir(directory).expect("read native source directory") {
        let path = entry.expect("native source entry").path();
        if path.is_dir() {
            collect(root, &path, paths);
        } else {
            paths.push(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}
fn main() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("../../..")
        .canonicalize()
        .unwrap();
    let mut paths = vec![
        "desktop/Cargo.toml",
        "desktop/Cargo.lock",
        "desktop/crates/runtime/Cargo.toml",
        "desktop/crates/protocol/Cargo.toml",
        "desktop/crates/daemon/Cargo.toml",
        "desktop/crates/runtime/build.rs",
        "src/kernelledger/worker.py",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    for directory in [
        "desktop/crates/runtime/src",
        "desktop/crates/protocol/src",
        "desktop/crates/daemon/src",
    ] {
        collect(&root, &root.join(directory), &mut paths);
    }
    paths.sort();
    let mut snapshot = Sha256::new();
    for path in paths {
        println!("cargo:rerun-if-changed={}", root.join(&path).display());
        snapshot.update(path.as_bytes());
        snapshot.update([0]);
        // Normalize checkout line endings so builds and package verification agree
        // across Git's Windows CRLF conversion and Linux checkouts.
        let bytes = fs::read(root.join(&path)).unwrap();
        let text = String::from_utf8(bytes)
            .expect("native build inputs are UTF-8")
            .replace("\r\n", "\n");
        snapshot.update(Sha256::digest(text.as_bytes()));
    }
    println!(
        "cargo:rustc-env=KL_NATIVE_SOURCE_SHA256={:x}",
        snapshot.finalize()
    );
}
