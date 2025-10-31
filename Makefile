# Create .env file if it doesn't exist
$(shell [ ! -f .env ] && touch .env)

# Load environment variables from .env file (safe: file always exists)
include .env

# Defaults if not set in .env
export RUST_BACKTRACE ?= full
export RUST_LOG ?= info,nockchain=info,nockchain_libp2p_io=info,libp2p=info,libp2p_quic=info
export MINIMAL_LOG_FORMAT ?= true
export MINING_PKH ?= 9yPePjfWAdUnzaQKyxcRXKRa5PpUzKKEwtpECBZsUYt9Jd7egSDEWoV

# Optional generic cargo flags (empty by default)
CARGO_FLAGS ?=

# GPU build switch (off by default). Usage: make GPU=1 install-nockchain
GPU ?= 0
CARGO_FEATURES :=
ifeq ($(GPU),1)
  CARGO_FEATURES += --features gpu
endif

# Utility macro to print key env vars
define show_env_vars
	@echo "RUST_LOG=$(RUST_LOG)"
	@echo "RUST_BACKTRACE=$(RUST_BACKTRACE)"
	@echo "MINIMAL_LOG_FORMAT=$(MINIMAL_LOG_FORMAT)"
	@echo "MINING_PKH=$(MINING_PKH)"
	@echo "GPU=$(GPU)  CARGO_FEATURES=$(CARGO_FEATURES)"
endef

.PHONY: build
build: build-hoon-all build-rust
	$(call show_env_vars)

## Build all rust (workspace)
.PHONY: build-rust
build-rust:
	@echo "==> cargo build --release $(CARGO_FLAGS) $(CARGO_FEATURES)"
	@cargo build --release $(CARGO_FLAGS) $(CARGO_FEATURES)

.PHONY: build-nockchain-jemalloc
build-nockchain-jemalloc:
	@echo "==> cargo build --release --features jemalloc --bin nockchain $(CARGO_FLAGS)"
	@cargo build --release --features jemalloc --bin nockchain $(CARGO_FLAGS)

## Tests
.PHONY: test
test:
	@cargo test --release $(CARGO_FLAGS) $(CARGO_FEATURES)

.PHONY: fmt
fmt:
	@cargo fmt

## Hoonc builds
.PHONY: build-hoon-all
build-hoon-all: build-hoonc

.PHONY: build-hoonc
build-hoonc: nuke-hoonc-data
	$(call show_env_vars)
	@echo "==> cargo build --release --locked --bin hoonc $(CARGO_FLAGS) $(CARGO_FEATURES)"
	@cargo build --release --locked --bin hoonc $(CARGO_FLAGS) $(CARGO_FEATURES)

.PHONY: build-hoonc-tracing
build-hoonc-tracing: nuke-hoonc-data
	$(call show_env_vars)
	@echo "==> cargo build --release --bin hoonc --features tracing-tracy $(CARGO_FLAGS) $(CARGO_FEATURES)"
	@cargo build --release --bin hoonc --features tracing-tracy $(CARGO_FLAGS) $(CARGO_FEATURES)

## Install binaries
.PHONY: install-hoonc
install-hoonc: nuke-hoonc-data
	$(call show_env_vars)
	@echo "==> cargo install --path crates/hoonc --locked --force $(CARGO_FLAGS) $(CARGO_FEATURES) --bin hoonc"
	@cargo install --path crates/hoonc --locked --force $(CARGO_FLAGS) $(CARGO_FEATURES) --bin hoonc

.PHONY: update-hoonc
update-hoonc:
	$(call show_env_vars)
	@echo "==> cargo install --locked --path crates/hoonc --bin hoonc"
	@cargo install --locked --path crates/hoonc --bin hoonc

.PHONY: build-nockchain
build-nockchain: assets/dumb.jam assets/miner.jam
	$(call show_env_vars)
	@echo "==> cargo build --release --bin nockchain --features tracing-tracy $(CARGO_FLAGS) $(CARGO_FEATURES)"
	@cargo build --release --bin nockchain --features tracing-tracy $(CARGO_FLAGS) $(CARGO_FEATURES)

.PHONY: install-nockchain
install-nockchain: assets/dumb.jam assets/miner.jam
	$(call show_env_vars)
	@echo "==> cargo install --path crates/nockchain --locked --force $(CARGO_FLAGS) $(CARGO_FEATURES) --bin nockchain"
	@cargo install --path crates/nockchain --locked --force $(CARGO_FLAGS) $(CARGO_FEATURES) --bin nockchain

## --- Stubs / helpers you referenced ---

.PHONY: nuke-hoonc-data
nuke-hoonc-data:
	@true
	# If you need to clear hoonc temp/artifacts, do it here.
	# Example:
	# rm -rf ./hoonc-data || true

# If these assets are generated, put the commands here.
# For now create empty files if missing.
assets/dumb.jam:
	@mkdir -p assets
	@[ -f $@ ] || touch $@

assets/miner.jam:
	@mkdir -p assets
	@[ -f $@ ] || touch $@
