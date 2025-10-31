use anyhow::Result;
use prover_hal::{ProverBackend, NttDir, Felt};

pub struct CpuBackend;

impl CpuBackend {
    pub fn new() -> Self { Self }
}

impl ProverBackend for CpuBackend {
    fn name(&self) -> &'static str { "cpu" }

    fn ntt_inplace(&mut self, poly: &mut [Felt], dir: NttDir) -> Result<()> {
        match dir {
            NttDir::Forward => {
                // call your existing CPU forward NTT
                // e.g., crate::ntt::forward(poly);
                todo!("wire to existing CPU forward NTT")
            }
            NttDir::Inverse => {
                // call your existing CPU inverse NTT
                // e.g., crate::ntt::inverse(poly);
                todo!("wire to existing CPU inverse NTT")
            }
        }
        // Ok(())
    }

    fn hash_many(&mut self, inputs: &[Felt], arity: usize, out: &mut [Felt]) -> Result<()> {
        // call your CPU Poseidon/RPO/SHA hash-many used for Merkle layers
        // out.len() must be inputs.len() / arity
        todo!("wire to existing CPU hash-many used in your Merkle builder")
    }
}
