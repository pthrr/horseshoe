use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Pinned ghostty commit — matches upstream libghostty-rs.
const GHOSTTY_REPO: &str = "https://github.com/ghostty-org/ghostty.git";
const GHOSTTY_COMMIT: &str = "bebca84668947bfc92b9a30ed58712e1c34eee1d";

fn main() {
    println!("cargo:rerun-if-env-changed=LIBGHOSTTY_VT_DIR");
    println!("cargo:rerun-if-env-changed=GHOSTTY_SOURCE_DIR");
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-env-changed=HOST");
    println!("cargo:rerun-if-changed=build.rs");

    // Pre-built library provided (e.g. by Nix) — skip the entire zig build.
    if let Ok(dir) = env::var("LIBGHOSTTY_VT_DIR") {
        let p = PathBuf::from(dir);
        let lib = p.join("lib");
        let inc = p.join("include");
        assert!(
            lib.join("libghostty-vt.a").exists(),
            "LIBGHOSTTY_VT_DIR missing lib/libghostty-vt.a: {}",
            p.display()
        );
        println!("cargo:rustc-link-search=native={}", lib.display());
        println!("cargo:rustc-link-lib=static=ghostty-vt");
        println!("cargo:include={}", inc.display());
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR must be set"));
    let target = env::var("TARGET").expect("TARGET must be set");

    // Locate ghostty source: env override > fetch into OUT_DIR.
    let ghostty_dir = match env::var("GHOSTTY_SOURCE_DIR") {
        Ok(dir) => {
            let p = PathBuf::from(dir);
            assert!(
                p.join("build.zig").exists(),
                "GHOSTTY_SOURCE_DIR does not contain build.zig: {}",
                p.display()
            );
            p
        }
        Err(_) => fetch_ghostty(&out_dir),
    };

    // Build libghostty-vt (both shared + static) via zig.
    // The upstream ghostty build.zig installs the static lib as libghostty-vt.a.
    let install_prefix = out_dir.join("ghostty-install");
    let host = env::var("HOST").expect("HOST must be set");
    let is_musl = target.contains("musl");
    let is_cross = target != host;

    let mut build = Command::new("zig");
    let _ = build
        .arg("build")
        .arg("-Demit-lib-vt")
        // Disable SIMD to avoid C++ deps (highway/simdutf use libc++).
        // Required for static linking since we don't link libc++/libstdc++.
        .arg("-Dsimd=false")
        .arg("--prefix")
        .arg(&install_prefix)
        .current_dir(&ghostty_dir);

    if is_musl {
        let _ = build.arg("--release=fast");
    }

    if is_cross || is_musl {
        let zig_target = zig_target(&target);
        let _ = build.arg(format!("-Dtarget={zig_target}"));
    }

    run(build, "zig build -Demit-lib-vt");

    let lib_dir = install_prefix.join("lib");
    let include_dir = install_prefix.join("include");

    let static_lib = lib_dir.join("libghostty-vt.a");
    assert!(
        static_lib.exists(),
        "expected static library at {}",
        static_lib.display()
    );
    assert!(
        include_dir.join("ghostty").join("vt.h").exists(),
        "expected header at {}",
        include_dir.join("ghostty").join("vt.h").display()
    );

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=ghostty-vt");
    println!("cargo:include={}", include_dir.display());
}

/// Clone ghostty at the pinned commit into OUT_DIR/ghostty-src.
/// Reuses an existing clone if the commit matches.
fn fetch_ghostty(out_dir: &Path) -> PathBuf {
    let src_dir = out_dir.join("ghostty-src");
    let stamp = src_dir.join(".ghostty-commit");

    // Skip fetch if we already have the right commit.
    if stamp.exists() {
        if let Ok(existing) = std::fs::read_to_string(&stamp) {
            if existing.trim() == GHOSTTY_COMMIT {
                return src_dir;
            }
        }
    }

    // Clean and clone fresh.
    if src_dir.exists() {
        std::fs::remove_dir_all(&src_dir)
            .unwrap_or_else(|e| panic!("failed to remove {}: {e}", src_dir.display()));
    }

    eprintln!("Fetching ghostty {GHOSTTY_COMMIT} ...");

    let mut clone = Command::new("git");
    let _ = clone
        .arg("clone")
        .arg("--filter=blob:none")
        .arg("--no-checkout")
        .arg(GHOSTTY_REPO)
        .arg(&src_dir);
    run(clone, "git clone ghostty");

    let mut checkout = Command::new("git");
    let _ = checkout
        .arg("checkout")
        .arg(GHOSTTY_COMMIT)
        .current_dir(&src_dir);
    run(checkout, "git checkout ghostty commit");

    std::fs::write(&stamp, GHOSTTY_COMMIT)
        .unwrap_or_else(|e| panic!("failed to write stamp: {e}"));

    src_dir
}

fn run(mut command: Command, context: &str) {
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("failed to execute {context}: {error}"));
    assert!(status.success(), "{context} failed with status {status}");
}

fn zig_target(target: &str) -> String {
    let value = match target {
        "x86_64-unknown-linux-gnu" => "x86_64-linux-gnu",
        "x86_64-unknown-linux-musl" => "x86_64-linux-musl",
        "aarch64-unknown-linux-gnu" => "aarch64-linux-gnu",
        "aarch64-unknown-linux-musl" => "aarch64-linux-musl",
        "aarch64-apple-darwin" => "aarch64-macos-none",
        "x86_64-apple-darwin" => "x86_64-macos-none",
        other => panic!("unsupported Rust target for vendored build: {other}"),
    };
    value.to_owned()
}
