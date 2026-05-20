use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let exe_name = if cfg!(windows) { "rhwp.exe" } else { "rhwp" };
    let built = manifest_dir
        .join("vendor/rhwp/target/release")
        .join(exe_name);
    println!("cargo:rerun-if-changed={}", built.display());
    if !built.exists() {
        println!(
            "cargo:warning=packaged rhwp binary was not found; run scripts/prepare_rhwp_binary.py before building wheels"
        );
        return;
    }

    let bin_dir = manifest_dir.join("src/kdsnr_hwp_toolkit/bin");
    if bin_dir.join(format!("{}.xz", exe_name)).exists() {
        return;
    }
    fs::create_dir_all(&bin_dir).expect("failed to create Python package bin dir");
    fs::copy(&built, bin_dir.join(exe_name)).expect("failed to copy rhwp into Python package");
}
