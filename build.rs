use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let ico = manifest.join("assets").join("icon.ico");
    println!("cargo:rerun-if-changed={}", ico.display());
    println!("cargo:rerun-if-changed=build.rs");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let rc = out.join("icon.rc");
    let obj = out.join("icon.o");
    let ico_fwd = ico.display().to_string().replace('\\', "/");
    std::fs::write(&rc, format!("1 ICON \"{ico_fwd}\"\n")).unwrap();

    let windres = env::var("WINDRES").unwrap_or_else(|_| "x86_64-w64-mingw32-windres".to_string());
    let status = Command::new(&windres)
        .arg(&rc)
        .arg("-O")
        .arg("coff")
        .arg("-o")
        .arg(&obj)
        .status()
        .expect("failed to run windres");
    assert!(status.success(), "windres failed to compile icon resource");

    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-arg-bins={}", obj.display());
}