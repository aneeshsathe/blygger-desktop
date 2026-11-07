//! Links libghostty-vt only when the `libghostty` feature is on. The default
//! build (the Alacritty engine) does nothing here: no Zig, no network.

fn main() {
    println!("cargo:rerun-if-env-changed=GHOSTTY_VT_LIB_DIR");
    if std::env::var_os("CARGO_FEATURE_LIBGHOSTTY").is_none() {
        return;
    }
    let Some(dir) = std::env::var_os("GHOSTTY_VT_LIB_DIR") else {
        panic!(
            "the libghostty feature links libghostty-vt 1.0: set GHOSTTY_VT_LIB_DIR to the \
             directory holding libghostty-vt.a (built from Ghostty's vt-1.0.0 tag with Zig). \
             Burrow never downloads it."
        );
    };
    println!(
        "cargo:rustc-link-search=native={}",
        std::path::PathBuf::from(dir).display()
    );
    println!("cargo:rustc-link-lib=static=ghostty-vt");
}
