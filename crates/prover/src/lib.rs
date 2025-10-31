pub mod backend; // add this line

pub use backend::CpuBackend;

// If you already export a prover builder, add a helper that accepts any backend:
// pub fn prove_with_backend<B: ProverBackend>(backend: &mut B, ...) -> Result<Proof> { ... }
