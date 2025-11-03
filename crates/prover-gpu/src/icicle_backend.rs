// crates/prover-gpu/src/icicle_backend.rs
use anyhow::{bail, Result};
use prover_hal::{Felt, NttDir, ProverBackend};

// If you want to keep the types handy for a future GPU impl:
#[allow(unused_imports)]
use icicle_runtime::{Device};
#[allow(unused_imports)]
use icicle_runtime::memory::DeviceVec;

pub struct IcicleBackend {
    #[allow(dead_code)]
    device: Option<Device>,
}

impl IcicleBackend {
    pub fn new() -> Result<Self> {
        // TODO: select/create a device when you wire the real kernels
        Ok(Self { device: None })
    }
}

impl ProverBackend for IcicleBackend {
    fn name(&self) -> &'static str { "gpu-icicle" }

    fn ntt_inplace_with_root(
        &mut self,
        _poly: &mut [Felt],
        _dir: NttDir,
        _root: Felt,
    ) -> Result<()> {
        // TEMP: return an error so the higher-level wrapper falls back to CPU.
        // Your fpoly/bpoly wrappers use `.ok()?` so an Err here triggers CPU fallback cleanly.
        bail!("ICICLE NTT not wired yet")
    }

    fn ntt_batched_with_root(
        &mut self,
        _polys: &mut [&mut [Felt]],
        _dir: NttDir,
        _root: Felt,
    ) -> Result<()> {
        bail!("ICICLE batched NTT not wired yet")
    }

    fn hash_many(&mut self, _inputs: &[Felt], _arity: usize, _out: &mut [Felt]) -> Result<()> {
        bail!("ICICLE hash_many not wired yet")
    }
}
