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
