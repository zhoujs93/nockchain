use prover_hal::{ProverBackend, NttDir, Felt};
use anyhow::Result;

#[cfg(feature = "cuda-ptx")]
mod cuda_backend;
#[cfg(feature = "icicle")]
mod icicle_backend;

pub enum WhichGpuBackend {
    #[cfg(feature = "cuda-ptx")]
    CudaPtx,
    #[cfg(feature = "icicle")]
    Icicle,
}

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

    fn ntt_inplace(&mut self, poly: &mut [Felt], dir: NttDir) -> Result<()> {
        self.inner.ntt_inplace(poly, dir)
    }

    fn ntt_batched(&mut self, polys: &mut [&mut [Felt]], dir: NttDir) -> Result<()> {
        self.inner.ntt_batched(polys, dir)
    }

    fn hash_many(&mut self, inputs: &[Felt], arity: usize, out: &mut [Felt]) -> Result<()> {
        self.inner.hash_many(inputs, arity, out)
    }
}
