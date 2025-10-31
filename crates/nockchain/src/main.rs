use std::error::Error;

use clap::Parser;
use kernels::dumb::KERNEL;
use nockapp::kernel::boot;
use nockapp::NockApp;
use nockchain::NockchainAPIConfig;
use zkvm_jetpack::hot::produce_prover_hot_state;

// When enabled, use jemalloc for more stable memory allocation
#[cfg(all(feature = "jemalloc", not(feature = "tracing-heap")))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(feature = "tracing-heap")]
#[global_allocator]
static ALLOC: tracy_client::ProfiledAllocator<tikv_jemallocator::Jemalloc> =
    tracy_client::ProfiledAllocator::new(tikv_jemallocator::Jemalloc, 100);

// ---- backend types & registry ----
use prover_hal::ProverBackend;
use nockchain_math::accel::install_backend;
use prover::CpuBackend;
#[cfg(feature = "gpu")]
use prover_gpu::GpuBackend;

// ---- CLI structure: flatten existing NockchainCli + GPU flags ----
#[derive(Parser, Debug)]
#[command(name = "nockchain")]
pub struct Args {
    #[command(flatten)]
    pub cli: nockchain::NockchainCli,

    /// Enable the GPU prover backend (requires building with --features gpu)
    #[arg(long, default_value_t = false)]
    pub gpu: bool,

    /// Comma-separated device list (e.g., "0" or "0,1")
    #[arg(long, default_value = "0")]
    pub gpu_devices: String,

    /// Batch size for GPU NTT/hash steps (if your backend uses it)
    #[arg(long, default_value_t = 1024)]
    pub gpu_batch: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    nockvm::check_endian();

    // Parse all CLI in one shot
    let Args { cli, gpu, gpu_devices: _gpu_devices, gpu_batch: _gpu_batch } = Args::parse();

    // Tracing/init
    boot::init_default_tracing(&cli.nockapp_cli);

    // Construct the backend FIRST
    let backend: Box<dyn ProverBackend> = {
        #[cfg(feature = "gpu")]
        {
            if gpu {
                // TODO: thread _gpu_devices / _gpu_batch into your backend ctor if/when supported
                Box::new(GpuBackend::new_cuda()?)   // or: GpuBackend::new_icicle()?
            } else {
                Box::new(CpuBackend::new())
            }
        }
        #[cfg(not(feature = "gpu"))]
        {
            Box::new(CpuBackend::new())
        }
    };

    // Then install it globally so math (fp_ntt/bp_ntt) can find it
    let _ = install_backend(backend);

    // Boot the node
    let prover_hot_state = produce_prover_hot_state();
    let mut nockchain: NockApp = nockchain::init_with_kernel(
        cli,
        KERNEL,
        prover_hot_state.as_slice(),
        NockchainAPIConfig::DisablePublicServer,
    ).await?; // <-- keep the ?

    nockchain.run().await?;  // <-- and keep the ?
    Ok(())
}
