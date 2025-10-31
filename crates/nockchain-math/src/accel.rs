use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use std::sync::Arc;
use prover_hal::{ProverBackend, NttDir, Felt};

static BACKEND: OnceCell<Arc<RwLock<Box<dyn ProverBackend>>>> = OnceCell::new();

pub fn install_backend(b: Box<dyn ProverBackend>) -> bool {
    BACKEND.set(Arc::new(RwLock::new(b))).is_ok()
}

pub fn with_backend<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut dyn ProverBackend) -> R,
{
    let arc = BACKEND.get()?;
    let mut guard = arc.write();
    Some(f(guard.as_mut()))
}
