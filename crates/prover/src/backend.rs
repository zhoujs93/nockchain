use anyhow::Result;
use prover_hal::{ProverBackend, NttDir, Felt};

pub struct CpuBackend;
impl CpuBackend { pub fn new() -> Self { Self } }

impl ProverBackend for CpuBackend {
    fn name(&self) -> &'static str { "cpu" }

    fn ntt_inplace_with_root(&mut self, poly: &mut [Felt], _dir: NttDir, root: Felt) -> Result<()> {
        // In this codebase, the direction is encoded by WHICH root you pass.
        // So we can ignore `dir` and just use the provided `root`.
        let out = nockchain_math::fpoly::fp_ntt_cpu(poly, &root);
        // Same length guaranteed:
        poly.copy_from_slice(&out);
        Ok(())
    }

    fn ntt_batched_with_root(&mut self, polys: &mut [&mut [Felt]], dir: NttDir, root: Felt) -> Result<()> {
        for p in polys {
            self.ntt_inplace_with_root(*p, dir, root)?;
        }
        Ok(())
    }

    fn hash_many(&mut self, inputs: &[Felt], arity: usize, out: &mut [Felt]) -> Result<()> {
        // If you already have a CPU Poseidon/Merkle layer routine, call it here.
        // Otherwise, keep this unimplemented for now if the Merkle path isn't routed via the backend yet.
        // Example placeholder:
        anyhow::bail!("CPU hash_many not wired yet; safe to leave uncalled while you only offload NTT");
    }
}
