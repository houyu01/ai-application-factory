fn main() {
    // The desktop executable embeds the icon at compile time. Track its source
    // directory so `tauri dev` rebuilds the executable whenever an icon changes.
    println!("cargo:rerun-if-changed=icons");

    // `gen` is a repository-managed symlink to the ignored `dist/gen` tree so
    // mobile build artifacts do not enter source control. Tauri generates its
    // capability schemas below `gen/schemas` during every desktop build. Make
    // the symlink target exist first; otherwise macOS reports EEXIST while
    // recursively creating a directory through a dangling symlink.
    std::fs::create_dir_all("../dist/gen").expect("create Tauri generated-files directory");

    tauri_build::build()
}
