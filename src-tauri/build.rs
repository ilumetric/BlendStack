fn main() {
    // Icons are embedded into the executable; rebuild resources when they change.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
