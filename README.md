# Excaliosa

It's Excaliosa not Excaliosaaa.

A Rust CLI tool that converts Excalidraw JSON diagrams to PNG or SVG format.

## Installation

### From Source

```bash
git clone <repository>
cd excaliosa
cargo build --release
```

The binary will be available at `target/release/excaliosa`.

## Usage

### Basic Usage

Convert an Excalidraw JSON file to PNG:

```bash
excaliosa path/to/diagram.json
```

This will create a PNG file with the same name as the input (e.g., `diagram.png`).

### Custom Output Path

Specify a custom output file path:

```bash
excaliosa path/to/diagram.json -o output.png
excaliosa path/to/diagram.json --output my_diagram.png
```

To export as SVG, just use an `.svg` extension:

```bash
excaliosa path/to/diagram.json -o diagram.svg
```

### Command-line options

- FILE (positional): Path to the Excalidraw JSON file (required).
- -o, --output <FILE>: Output file path.
	- Default: same as input filename with a `.png` extension.
	- The output format is inferred from the extension: `.svg` for SVG, `.png` for PNG.
- --legacy: Use the legacy SVG-based renderer instead of the default `rough_tiny_skia` PNG renderer.
	- When PNG is requested and `--legacy` is set, the tool generates SVG first and then rasterizes it to PNG.
	- Helpful if you need output that mirrors the SVG pipeline or for troubleshooting differences between renderers.
- -h, --help: Show help and exit.

### More examples

```bash
# Default PNG next to input
excaliosa examples/arrows.json           # -> examples/arrows.png

# Explicit PNG path
excaliosa examples/arrows.json -o out.png

# Export SVG directly
excaliosa examples/arrows.json -o arrows.svg

# Render PNG using the legacy SVG pipeline
excaliosa examples/arrows.json --legacy -o legacy.png
```

### Help

```bash
excaliosa --help
```

## Example Workflow

```bash
# Create a diagram in Excalidraw
# Export it as JSON

# Convert to PNG
excaliosa my_diagram.json

# Or with custom output
excaliosa my_diagram.json -o diagrams/my_output.png
```

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
