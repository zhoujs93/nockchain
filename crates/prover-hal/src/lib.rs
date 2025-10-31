use anyhow::Result;

/// Replace with your actual field element type if different.
/// Many STARK stacks use Goldilocks (u64) in Montgomery form.
pub type Felt = u64;

#[derive(Copy, Clone, Debug)]
pub enum NttDir { Forward, Inverse }

pub trait ProverBackend: Send + Sync + 'static {
    fn name(&self) -> &'static str;

    fn ntt_inplace(&mut self, poly: &mut [Felt], dir: NttDir) -> Result<()>;
    fn ntt_batched(&mut self, polys: &mut [&mut [Felt]], dir: NttDir) -> Result<()>;

    /// Hash-many for Merkle layers (arity = 2/4/8/...).
    fn hash_many(&mut self, inputs: &[Felt], arity: usize, out: &mut [Felt]) -> Result<()>;
}

pub mod backend; // add this line

pub use backend::CpuBackend;

// If you already export a prover builder, add a helper that accepts any backend:
// pub fn prove_with_backend<B: ProverBackend>(backend: &mut B, ...) -> Result<Proof> { ... }
