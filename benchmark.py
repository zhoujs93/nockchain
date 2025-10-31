#!/usr/bin/env python3
import os, re, time, shlex, subprocess, tempfile, pathlib, statistics, json
from datetime import datetime

BIN = os.path.expanduser("~/.cargo/bin/nockchain")
RUN_ROOT = pathlib.Path.home() / "nock_bench_runs"  # separate from your prod run1/
RUN_ROOT.mkdir(parents=True, exist_ok=True)

# Common CLI bits you use in prod; add peers you want for reliable %mine effects.
COMMON_ARGS = [
    "--mine",
    "--mining-pkh", "7zACBdiqSrsE1DeE2ytKrdY1aKrrmQsaDBHunpTYz9FRtihGe84YyPd",
    "--bind", "/ip4/0.0.0.0/udp/3006/quic-v1",
    "--no-new-peer-id",
    "--bind-private-grpc-port", "5556",
    "--bind-public-grpc-addr", "127.0.0.1:5557",
    # Use a snapshot if you like; adjust relative to RUN_DIR per trial:
    # "--state-jam", "../assets/16455.jam",
    "--peer", "/dnsaddr/nockchain-backbone.zorp.io",
    # Add 1–2 static boot peers for stability (example):
    # "--peer", "/ip4/34.129.68.86/udp/33000/quic-v1/p2p/12D3KooWEDMUcShVM1jJ19MCZvKsSVFmucjih4xRivc2Q2Fp7i8a",
]

# Regexes / signals
RE_ATTEMPT = re.compile(r"starting mining attempt on thread", re.IGNORECASE)
RE_THREADS_STARTED = re.compile(r"mining threads started with\s+(\d+)\s+threads", re.IGNORECASE)
RE_MINE_EFFECT = re.compile(r"\breceived new candidate block header\b|\[%mining-on\b", re.IGNORECASE)

def run_trial(threads: int, measure_s: int = 180, warmup_s: int = 45):
    trial_dir = RUN_ROOT / f"threads_{threads}_{int(time.time())}"
    trial_dir.mkdir(parents=True, exist_ok=True)
    # Keep state across trials to avoid full resync; do NOT rm -rf .data.nockchain.
    (trial_dir / ".socket").mkdir(exist_ok=True)
    # Clean stale socket only:
    sock = trial_dir / ".socket" / "nockchain_npc.sock"
    try: sock.unlink()
    except FileNotFoundError: pass

    env = os.environ.copy()
    env["RUST_LOG"] = "info,nockchain=debug,mining=debug"  # verbose mining
    env["MINIMAL_LOG_FORMAT"] = "1"
    env["RUST_BACKTRACE"] = "full"

    cmd = [BIN, "--num-threads", str(threads)] + COMMON_ARGS
    # If using --state-jam relative path, make sure it's valid from trial_dir
    print(f"[{datetime.now().isoformat(timespec='seconds')}] starting trial {threads} threads")
    print("CMD:", " ".join(shlex.quote(x) for x in cmd))
    p = subprocess.Popen(cmd, cwd=str(trial_dir), env=env,
                         stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                         text=True, bufsize=1)

    attempts = 0
    seen_threads_started = False
    seen_mine_effect = False
    t0 = time.time()
    warm_started = None
    measure_started = None
    lines_buffer = []

    try:
        while True:
            line = p.stdout.readline()
            if not line:
                if p.poll() is not None:
                    raise RuntimeError(f"miner exited early with code {p.returncode}")
                continue

            lines_buffer.append(line)
            if RE_MINE_EFFECT.search(line):
                seen_mine_effect = True
            if RE_THREADS_STARTED.search(line):
                seen_threads_started = True

            # We only count attempts once mining threads are actually running
            if RE_ATTEMPT.search(line) and seen_threads_started:
                attempts += 1

            now = time.time()

            # Start warmup after we see mining threads or after a hard timeout
            if warm_started is None and (seen_threads_started or now - t0 > 90):
                warm_started = now

            # Switch to measure window after warmup
            if warm_started and measure_started is None and (now - warm_started >= warmup_s):
                measure_started = now
                # reset attempts for the measure window
                attempts = 0

            # End trial after measure_s elapsed
            if measure_started and (now - measure_started >= measure_s):
                break

    finally:
        # Try graceful shutdown; if it lingers, kill
        p.terminate()
        try:
            p.wait(timeout=10)
        except subprocess.TimeoutExpired:
            p.kill()

    # Compute attempts/min over the measured window
    window = (time.time() - measure_started) if measure_started else 1.0
    apm = attempts * (60.0 / window)
    result = {
        "threads": threads,
        "attempts": attempts,
        "window_s": round(window, 1),
        "attempts_per_min": round(apm, 3),
        "saw_mine_effect": seen_mine_effect,
        "saw_threads_started": seen_threads_started,
        "trial_dir": str(trial_dir),
    }

    # Save raw log for this trial
    (trial_dir / "stdout.log").write_text("".join(lines_buffer))
    (trial_dir / "result.json").write_text(json.dumps(result, indent=2))
    print(f"[{datetime.now().isoformat(timespec='seconds')}] done: {result}")
    return result

if __name__ == "__main__":
    settings = [64, 80, 92]
    results = []
    for t in settings:
        results.append(run_trial(t, measure_s=180, warmup_s=45))

    # Simple ranking
    results.sort(key=lambda r: r["attempts_per_min"], reverse=True)
    print("\n=== Ranking by attempts/min ===")
    for r in results:
        print(f"{r['threads']:>3} threads  →  {r['attempts_per_min']:.2f} APM "
              f"(attempts={r['attempts']}, window={r['window_s']}s)")
