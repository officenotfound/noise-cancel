//! Embeds the application icon on Windows: assets/quietmic.res (see scripts/make_win_icon.py) goes straight to the linker.
fn main() {
    println!("cargo:rerun-if-changed=../../assets/quietmic.res");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let res = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../assets/quietmic.res");
        println!("cargo:rustc-link-arg-bins={}", res.display());
    }
}
