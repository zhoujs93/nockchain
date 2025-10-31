use anyhow::Result;

/// Replace with your actual field element type if different.
pub type Felt = u64;

#[derive(Copy, Clone, Debug)]
pub enum NttDir { Forward, Inverse }

pub trait ProverBackend: Send + Sync + 'static {
    fn name(&self) -> &'static str;

    // UPDATED: include `root` so the backend can use the right domain
    fn ntt_inplace_with_root(&mut self, poly: &mut [Felt], dir: NttDir, root: Felt) -> anyhow::Result<()>;

    fn ntt_batched_with_root(&mut self, polys: &mut [&mut [Felt]], dir: NttDir, root: Felt) -> anyhow::Result<()>;

    fn hash_many(&mut self, inputs: &[Felt], arity: usize, out: &mut [Felt]) -> anyhow::Result<()>;
}

impl ProverBackend for CpuBackend {
    fn name(&self) -> &'static str { "cpu" }

    fn ntt_inplace_with_root(&mut self, poly: &mut [Felt], dir: NttDir, root: Felt) -> anyhow::Result<()> {
        match dir {
            NttDir::Forward => {
                // your existing CPU forward NTT that already takes `root`
                crate::fpoly::fp_ntt_inplace(poly, &root); // example — use your real function
            }
            NttDir::Inverse => {
                crate::fpoly::fp_intt_inplace(poly, &root); // example — use your real function
            }
        }
        Ok(())
    }

    fn ntt_batched_with_root(&mut self, polys: &mut [&mut [Felt]], dir: NttDir, root: Felt) -> anyhow::Result<()> {
        for p in polys { self.ntt_inplace_with_root(p, dir, root)?; }
        Ok(())
    }

    fn hash_many(&mut self, inputs: &[Felt], arity: usize, out: &mut [Felt]) -> anyhow::Result<()> {
        // call your CPU Poseidon/RPO "hash-many" used to build Merkle layers
        // out.len() must equal inputs.len()/arity
        crate::merkle::poseidon_hash_many(inputs, arity, out); // example
        Ok(())
    }
}
