.PHONY: build fmt fmt-check clippy test-docs test-unit test-prepare test clean

BINARY_NAME := gamchi
BIN_DIR := bin
CARGO_BIN := target/debug/$(BINARY_NAME)

build:
	cargo build -p gamchi
	mkdir -p $(BIN_DIR)
	cp $(CARGO_BIN) $(BIN_DIR)/$(BINARY_NAME)

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test-docs:
	cargo run -p docscheck --bin docscheck -q

test-unit:
	cargo test --workspace

test-prepare:
	$(MAKE) fmt-check
	$(MAKE) test-docs
	$(MAKE) clippy

test:
	$(MAKE) test-prepare
	$(MAKE) test-unit

clean:
	rm -rf $(BIN_DIR) target
