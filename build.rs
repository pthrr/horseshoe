use std::env;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();

    // For musl targets, provide the search path for static libxkbcommon.
    // Use target-specific env var first (for cross-compilation), then generic.
    if target.contains("musl") {
        let target_upper = target.replace('-', "_").to_uppercase();
        let target_var = format!("XKBCOMMON_LIB_DIR_{target_upper}");
        let xkb_lib = env::var(&target_var)
            .or_else(|_| env::var("XKBCOMMON_LIB_DIR"))
            .ok();
        if let Some(dir) = xkb_lib {
            println!("cargo:rustc-link-search=native={dir}");
        }
        println!("cargo:rerun-if-env-changed={target_var}");
    }

    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-env-changed=XKBCOMMON_LIB_DIR");
    println!("cargo:rerun-if-changed=build.rs");
}
