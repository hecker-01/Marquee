//! Embeds the application manifest (per-monitor DPI awareness v2) on Windows.

fn main() {
    let manifest = "packaging/windows/marquee.manifest";
    println!("cargo:rerun-if-changed={manifest}");
    let windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows_msvc {
        let path = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join(manifest);
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", path.display());
    }
}
