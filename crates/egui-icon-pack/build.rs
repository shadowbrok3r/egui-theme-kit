#[cfg(feature = "phosphor-regular")]
fn main() -> Result<(), egui_icon_pack_build::Error> {
    egui_icon_pack_build::IconPackBuild {
        uri_prefix: "egui-icon-pack/phosphor-regular".into(),
        runtime_path: "crate".into(),
        ..egui_icon_pack_build::IconPackBuild::in_out_dir("assets/phosphor", "phosphor_regular.rs")
    }
    .generate()?;
    Ok(())
}

#[cfg(not(feature = "phosphor-regular"))]
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
}
