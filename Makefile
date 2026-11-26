.PHONY: build release test test-cli clean install help fmt lint

# Default target
help:
	@echo "Excaliosa - Excalidraw JSON to PNG Converter"
	@echo ""
	@echo "Available targets:"
	@echo "  build       - Build the project in debug mode"
	@echo "  release     - Build the project in release mode"
	@echo "  test        - Run tests"
	@echo "  test-cli    - Test the CLI with sample diagrams"
	@echo "  clean       - Clean build artifacts"
	@echo "  install     - Install the binary to ~/.cargo/bin/"
	@echo "  fmt         - Format code with rustfmt"
	@echo "  lint        - Run clippy linter"

# Build targets
build:
	cargo build

release:
	cargo build --release

test:
	cargo test

test-cli: release
	@set -eu; \
	dir=$$(mktemp -d target/cli-smoke.XXXXXX); \
	trap 'rm -f "$$dir/arrows.json" "$$dir/arrows.png" "$$dir/roughness.png" "$$dir/arrows.svg"; rmdir "$$dir"' EXIT; \
	cp examples/arrows.json "$$dir/arrows.json"; \
	./target/release/excaliosa "$$dir/arrows.json"; \
	./target/release/excaliosa examples/test_roughness.json -o "$$dir/roughness.png"; \
	./target/release/excaliosa examples/arrows.json -o "$$dir/arrows.svg"; \
	for png in "$$dir/arrows.png" "$$dir/roughness.png"; do \
		test "$$(od -An -tx1 -N8 "$$png" | tr -d ' \n')" = 89504e470d0a1a0a; \
	done; \
	grep -q '^<svg ' "$$dir/arrows.svg"

clean:
	cargo clean

install: release
	cp target/release/excaliosa ~/.cargo/bin/

fmt:
	cargo fmt

lint:
	cargo clippy -- -D warnings
