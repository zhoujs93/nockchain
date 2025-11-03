# Remote VM

ssh -i ~/.ssh/google_cloud_compute_putsncalls23 putsncalls23@78.46.165.58

# Wallet 

Address
7zACBdiqSrsE1DeE2ytKrdY1aKrrmQsaDBHunpTYz9FRtihGe84YyPd

Extended Private Key (save this for import)
zprvLxxkCBq3s5HYzjtN8fveJJinp48RUZagBCF23aNC4hd1QscWS7kkXkPT97XQkpBss3ffv4pR8A3MXjNHuc4KVxVD4ZJeFLVUCtQw3z1PNUpT

Extended Public Key (save this for import)
zpub2kRJ7D6VCvzVfDh2fzuwCouqQtE6MBvKMVpQD5bmZyrktskipSVobVcNCBNvTESqv8ZaxTXoBRsKoiaxLgqkd2WdGk7ebVN3r9wH6MWSeobcAhRhU8TdoqyA2gyQdXSxEfwDxuJDhPxEpDLcjni6
5gbdwfDQc5tcjYscF3RJ33HeDRMnTKsGjh3WHn7aqmw9kvae

Seed Phrase (save this for import)
'fringe then trip reward stuff fine deny cash blush speed bullet negative subway shop frown analyst train issue valid lumber only scene salute adjust'

Version (keep this for import with seed phrase)
1

nockchain --mine   --bind /ip4/0.0.0.0/udp/31001/quic-v1 --mining-pkh 5uu1BeNRMdqrvwTf7Rqf3ZeDp3kNicgECPSMuJjaFPx2cYXEUy5C8nR

nockchain --mining-pkh 7zACBdiqSrsE1DeE2ytKrdY1aKrrmQsaDBHunpTYz9FRtihGe84YyPd --mine --bind /ip4/0.0.0.0/udp/31001/quic-v1 --num-threads 92 --peer /ip4/34.129.68.86/udp/33000/quic-v1 --peer /ip4/34.174.118.156/udp/33000/quic-v1 --peer /ip4/34.16.171.87/udp/33000/quic-v1 --peer /ip4/216.82.192.27/udp/3336/quic-v1

# logging

journalctl -o cat -fu nock_miner --since "2 hours ago" | grep --line-buffered -E "Starting mining driver|mining threads (started|starting)|Found block|received new candidate block header|restarting mining attempts|driver 'mining' initialized|all drivers initialized, born poke sent|poke (acknowledged|nacked)|set-mining-key-advanced|Error receiving effect|Expected (effect|two elements|three elements)"

# Compiling with GPU:

sudo apt-get update
sudo apt-get install -y libstdc++-12-dev libstdc++-11-dev

# make sure compilers and CUDA env are in THIS shell
export CC=/usr/bin/gcc
export CXX=/usr/bin/g++
export CUDA_HOME=/usr/local/cuda
export PATH="$CUDA_HOME/bin:$PATH"
export LD_LIBRARY_PATH="$CUDA_HOME/lib64:${LD_LIBRARY_PATH:-}"
export ICICLE_CUDA_ARCH=89
export CUDAARCHS=89

# nuke ICICLE’s cached build dirs so CMake reconfigures
rm -rf target/release/build/icicle-runtime-*/out/build \
       target/release/build/icicle-core-*/out/build

# rebuild with verbose output
RUST_BACKTRACE=1 make GPU=1 BACKEND=icicle install-nockchain V=1
