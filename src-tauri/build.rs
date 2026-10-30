fn main() {
    // Icons are embedded into the executable; rebuild resources when they change.
    println!("cargo:rerun-if-changed=icons");
    // The .blend file icon, resource 2: `FILE_ICON` in src/assoc.rs points at it.
    let file_icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/blend-file.ico");
    let windows = tauri_build::WindowsAttributes::new().append_rc_content(format!(
        "2 ICON \"{}\"",
        file_icon.display().to_string().replace('\\', "/")
    ));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri-build");
}
