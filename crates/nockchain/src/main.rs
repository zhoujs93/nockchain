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

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "nockchain")]
pub struct Args {
    // ... existing options ...
    /// Enable the GPU prover backend (requires building with --features gpu)
    #[arg(long, default_value_t = false)]
    pub gpu: bool,

    /// Comma-separated device list (e.g., "0" or "0,1")
    #[arg(long, default_value = "0")]
    pub gpu_devices: String,

    /// Batch size for GPU NTT/hash steps
    #[arg(long, default_value_t = 1024)]
    pub gpu_batch: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    nockvm::check_endian();
    let cli = nockchain::NockchainCli::parse();
    boot::init_default_tracing(&cli.nockapp_cli);

    let prover_hot_state = produce_prover_hot_state();
    let mut nockchain: NockApp = nockchain::init_with_kernel(
        cli,
        KERNEL,
        prover_hot_state.as_slice(),
        NockchainAPIConfig::DisablePublicServer,
    )
    .await?;
    nockchain.run().await?;
    Ok(())
}
