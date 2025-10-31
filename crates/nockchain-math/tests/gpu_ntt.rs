[dev-dependencies]
prover-gpu = { path = "../prover-gpu" }   # needed only for tests
prover-hal = { path = "../prover-hal" }   # for Felt/NttDir in tests

#[cfg(all(test, feature = "gpu"))]
mod tests {
    use super::*;
    use prover_hal::{Felt, NttDir};
    use nockchain_math::accel::install_backend;
    use prover_gpu::GpuBackend;

    #[test]
    fn fp_ntt_gpu_matches_cpu() {
        // Pick a size you know is supported. 1024 is usually fine.
        // Use the *same* function you use in production to compute the 1024-th root.
        // If you don't have a helper yet, pass in the exact root you already use elsewhere.
        let root: Felt = /* TODO: your project's 1024-th primitive root */;

        let src: Vec<Felt> = (0..1024).map(|i| i as Felt).collect();

        // CPU baseline: call the CPU-only function to avoid GPU recursion.
        let cpu = fp_ntt_cpu(&src, &root);

        // Install GPU backend for this test (CUDA path shown)
        install_backend(Box::new(GpuBackend::new_cuda().unwrap()));

        // GPU path is the public wrapper
        let gpu = fp_ntt(&src, &root);

        assert_eq!(cpu, gpu);
    }
}
