fn main() {
    // Icons are embedded into the executable; rebuild resources when they change.
    println!("cargo:rerun-if-changed=icons");
    // The .blend file icon, resource 32513: `FILE_ICON` in src/assoc.rs points at it.
    // Windows shows the icon with the lowest id as the exe's own, and Tauri's
    // app icon is 32512, so this one has to come after it.
    let file_icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/blend-file.ico");
    let windows = tauri_build::WindowsAttributes::new().append_rc_content(format!(
        "32513 ICON \"{}\"",
        file_icon.display().to_string().replace('\\', "/")
    ));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri-build");
}
