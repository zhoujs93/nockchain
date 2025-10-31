use anyhow::Result;
use prover_hal::{ProverBackend, NttDir, Felt};

#[cfg(feature = "cuda-ptx")]
mod cuda_backend;
#[cfg(feature = "icicle")]
mod icicle_backend;

pub struct GpuBackend {
    inner_name: &'static str,
    inner: Box<dyn ProverBackend>,
}

impl GpuBackend {
    #[cfg(feature = "cuda-ptx")]
    pub fn new_cuda() -> Result<Self> {
        let b = cuda_backend::CudaBackend::new()?;
        Ok(Self { inner_name: "gpu-cuda", inner: Box::new(b) })
    }

    #[cfg(feature = "icicle")]
    pub fn new_icicle() -> Result<Self> {
        let b = icicle_backend::IcicleBackend::new()?;
        Ok(Self { inner_name: "gpu-icicle", inner: Box::new(b) })
    }
}

impl ProverBackend for GpuBackend {
    fn name(&self) -> &'static str { self.inner_name }

    fn ntt_inplace_with_root(&mut self, poly: &mut [Felt], dir: NttDir, root: Felt) -> Result<()> {
        self.inner.ntt_inplace_with_root(poly, dir, root)
    }

    fn ntt_batched_with_root(&mut self, polys: &mut [&mut [Felt]], dir: NttDir, root: Felt) -> Result<()> {
        self.inner.ntt_batched_with_root(polys, dir, root)
    }

    fn hash_many(&mut self, inputs: &[Felt], arity: usize, out: &mut [Felt]) -> Result<()> {
        self.inner.hash_many(inputs, arity, out)
    }
}
