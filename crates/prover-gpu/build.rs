// Compiles CUDA kernels to PTX at build time.
use std::{env, process::Command, path::PathBuf};

fn main() {
    // Skip if not building the CUDA path.
    if env::var("CARGO_FEATURE_CUDA_PTX").is_err() { return; }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ptx_out = out_dir.join("ntt.ptx");

    // Require nvcc in PATH
    let status = Command::new("nvcc")
        .args([
            "-ptx", "kernels/ntt.cu",
            "-o", ptx_out.to_str().unwrap(),
            "-Xcompiler", "-fPIC",
        ])
        .status()
        .expect("failed to run nvcc; ensure CUDA toolkit is installed");

    if !status.success() {
        panic!("nvcc failed; see build output for details");
    }

    println!("cargo:rerun-if-changed=kernels/ntt.cu");
}
