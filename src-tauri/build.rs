fn main() {
    // The desktop executable embeds the icon at compile time. Track its source
    // directory so `tauri dev` rebuilds the executable whenever an icon changes.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
