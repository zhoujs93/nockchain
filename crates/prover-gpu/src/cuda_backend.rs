use anyhow::Result;
use cust::{prelude::*, memory::DeviceBuffer};
use prover_hal::{ProverBackend, NttDir, Felt};

pub struct CudaBackend {
    _ctx: Context,         // keep CUDA context alive
    module: Module,        // loaded PTX module
    stream: Stream,
    // TODO: keep reusable device buffers here to avoid re-allocs
}

impl CudaBackend {
    pub fn new() -> Result<Self> {
        cust::init(cust::CudaFlags::empty())?;
        let device = Device::get_device(0)?;
        let _ctx = Context::create_and_push(ContextFlags::SCHED_AUTO, device)?;

        // Load PTX compiled by build.rs
        let ptx = include_str!(concat!(env!("OUT_DIR"), "/ntt.ptx"));
        let module = Module::from_ptx(ptx, &[])?;
        let stream = Stream::new(StreamFlags::NON_BLOCKING, None)?;

        Ok(Self { _ctx, module, stream })
    }
}

impl ProverBackend for CudaBackend {
    fn name(&self) -> &'static str { "gpu-cuda" }

    fn ntt_inplace(&mut self, poly: &mut [Felt], dir: NttDir) -> Result<()> {
        // Demo: launch a placeholder kernel that currently copies the buffer.
        // Replace with your actual NTT kernel & twiddle handling.
        let func = self.module.get_function(match dir {
            NttDir::Forward => "ntt_forward",
            NttDir::Inverse => "ntt_inverse",
        })?;

        let n = poly.len();
        let mut d = DeviceBuffer::<Felt>::from_slice(poly)?;
        // launch with simple 1D grid/block:
        let block = 256;
        let grid = ((n as u32) + block - 1) / block;
        unsafe {
            launch!(func<<<grid, block, 0, self.stream>>>(
                d.as_device_ptr(),
                n as u32
            ))?;
        }
        self.stream.synchronize()?;
        d.copy_to(poly)?;
        Ok(())
    }

    fn ntt_batched(&mut self, polys: &mut [&mut [Felt]], dir: NttDir) -> Result<()> {
        for p in polys {
            self.ntt_inplace(p, dir)?;
        }
        Ok(())
    }

    fn hash_many(&mut self, _inputs: &[Felt], _arity: usize, _out: &mut [Felt]) -> Result<()> {
        // TODO: implement a GPU Poseidon (or your hash) kernel; for now, fall back to CPU or return Err.
        anyhow::bail!("hash_many not implemented in CUDA backend yet")
    }
}
