.PHONY: build release test clean install help

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
	@echo "Testing basic diagram conversion..."
	./target/release/excaliosa test_diagram.json
	@echo "Generated: test_diagram.png"
	@echo ""
	@echo "Testing complex diagram conversion..."
	./target/release/excaliosa test_diagram_complex.json
	@echo "Generated: test_diagram_complex.png"
	@echo ""
	@echo "Testing custom output..."
	./target/release/excaliosa test_diagram.json -o /tmp/excaliosa_test.png
	@echo "Generated: /tmp/excaliosa_test.png"
	@echo ""
	@echo "All tests completed successfully!"

clean:
	cargo clean
	rm -f test_diagram.png test_diagram_complex.png my_custom_output.png

install: release
	cp target/release/excaliosa ~/.cargo/bin/

fmt:
	cargo fmt

lint:
	cargo clippy -- -D warnings
