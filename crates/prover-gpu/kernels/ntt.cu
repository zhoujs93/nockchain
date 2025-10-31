extern "C" __global__
void ntt_forward(unsigned long long* buf, unsigned int n) {
    // TODO: implement butterfly steps over your field (e.g., Goldilocks) with twiddles in __constant__ memory.
    // For now, no-op copy style kernel (does nothing).
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) { buf[i] = buf[i]; }
}

extern "C" __global__
void ntt_inverse(unsigned long long* buf, unsigned int n) {
    unsigned int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) { buf[i] = buf[i]; }
}
