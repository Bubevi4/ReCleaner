fn main() {
    let config = slint_build::CompilerConfiguration::new()
        .with_bundled_translations("ui/lang")
        // .po files have no msgctxt; without this, @tr looks up by component name and misses.
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint compile failed");

    #[cfg(windows)]
    {
        ensure_placeholder_icon();
        println!("cargo:rerun-if-changed=resources/app.manifest");
        println!("cargo:rerun-if-changed=resources/app.rc");
        println!("cargo:rerun-if-changed=resources/icon.ico");
        println!("cargo:rerun-if-changed=ui/lang/en/LC_MESSAGES/app.po");
        println!("cargo:rerun-if-changed=ui/lang/ru/LC_MESSAGES/app.po");
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
