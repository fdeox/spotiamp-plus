fn main() {
    // tauri_build copies the bundled resources, but only watches the files
    // that were there when it last ran: a preset added to the folder wouldn't
    // be copied. A directory here makes cargo look through all of it.
    println!("cargo:rerun-if-changed=presets");
    println!("cargo:rerun-if-changed=milkdrop");
    println!("cargo:rerun-if-changed=textures");
    tauri_build::build()
}
