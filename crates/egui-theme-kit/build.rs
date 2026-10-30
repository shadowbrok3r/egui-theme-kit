// Sets `cfg(glass_backend)` for the `glass` feature, or for `eframe` on any target but iOS and wasm32.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-check-cfg=cfg(glass_backend)");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let native_gl = os != "ios" && arch != "wasm32";
    let eframe = std::env::var_os("CARGO_FEATURE_EFRAME").is_some();
    if (native_gl && eframe) || std::env::var_os("CARGO_FEATURE_GLASS").is_some() {
        println!("cargo:rustc-cfg=glass_backend");
    }
}
