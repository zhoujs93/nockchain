use anyhow::Result;
use prover_hal::{ProverBackend, NttDir, Felt};

/// Wrap your existing CPU NTT/hash primitives here.
/// Replace the `todo!()` stubs with calls into your current prover code.
pub struct CpuBackend;

impl CpuBackend {
    pub fn new() -> Self { Self }
}

impl ProverBackend for CpuBackend {
    fn name(&self) -> &'static str { "cpu" }

    fn ntt_inplace(&mut self, poly: &mut [Felt], dir: NttDir) -> Result<()> {
        match dir {
            NttDir::Forward => {
                // call your current CPU forward NTT
                // e.g., crate::ntt::forward(poly)
                todo!("wire to existing CPU forward NTT")
            }
            NttDir::Inverse => {
                // call your current CPU inverse NTT
                todo!("wire to existing CPU inverse NTT")
            }
        }
    }

    fn ntt_batched(&mut self, polys: &mut [&mut [Felt]], dir: NttDir) -> Result<()> {
        for p in polys.iter_mut() {
            self.ntt_inplace(p, dir)?;
        }
        Ok(())
    }

    fn hash_many(&mut self, inputs: &[Felt], arity: usize, out: &mut [Felt]) -> Result<()> {
        // call your CPU Poseidon/RPO/SHA hash-many used for Merkle layers
        // out.len() should be inputs.len() / arity.
        todo!("wire to existing CPU hash-many used in your Merkle builder")
    }
}
