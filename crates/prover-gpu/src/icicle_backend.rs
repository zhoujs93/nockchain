use anyhow::Result;
use prover_hal::{ProverBackend, NttDir, Felt};

// Pseudo imports; pick the right field module & APIs from ICICLE.
// Example names; confirm in your environment.
use icicle_runtime::{DeviceVec};
use icicle_core::ntt::{NTTConfig, ntt_inplace as icicle_ntt_inplace};

pub struct IcicleBackend {
    cfg: NTTConfig, // tune batch size/streams later
}

impl IcicleBackend {
    pub fn new() -> Result<Self> {
        // Initialize once; select device 0, set stream count, etc.
        Ok(Self { cfg: NTTConfig::default() })
    }
}

impl ProverBackend for IcicleBackend {
    fn name(&self) -> &'static str { "gpu-icicle" }

    fn ntt_inplace(&mut self, poly: &mut [Felt], dir: NttDir) -> Result<()> {
        // Copy to device; call ICICLE NTT; copy back.
        let mut d = DeviceVec::<Felt>::from_slice(poly)?;
        let forward = matches!(dir, NttDir::Forward);
        icicle_ntt_inplace(&mut d, forward, &self.cfg)?;
        d.copy_to(poly)?;
        Ok(())
    }

    fn ntt_batched(&mut self, polys: &mut [&mut [Felt]], dir: NttDir) -> Result<()> {
        // (Optional) pack into a single device buffer and call ICICLE batched NTT.
        for p in polys { self.ntt_inplace(p, dir)?; }
        Ok(())
    }

    fn hash_many(&mut self, _inputs: &[Felt], _arity: usize, _out: &mut [Felt]) -> Result<()> {
        // Use ICICLE Poseidon hash-many API for your arity.
        // icicle_poseidon::hash_many(...)
        anyhow::bail!("hash_many not wired yet; connect ICICLE Poseidon here")
    }
}
