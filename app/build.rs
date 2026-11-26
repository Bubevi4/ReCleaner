fn main() {
    let config = slint_build::CompilerConfiguration::new().with_bundled_translations("ui/lang");
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint compile failed");

    #[cfg(windows)]
    {
        ensure_placeholder_icon();
        println!("cargo:rerun-if-changed=resources/app.manifest");
        println!("cargo:rerun-if-changed=resources/app.rc");
        println!("cargo:rerun-if-changed=resources/icon.ico");
        let _ = embed_resource::compile("resources/app.rc", embed_resource::NONE);
    }
}

#[cfg(windows)]
fn ensure_placeholder_icon() {
    let ico = std::path::Path::new("resources/icon.ico");
    if ico.exists() {
        return;
    }
    let placeholder = std::path::Path::new("resources/placeholder.ico");
    if placeholder.exists() {
        let _ = std::fs::copy(placeholder, ico);
    }
}
