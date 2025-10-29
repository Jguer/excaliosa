# Excaliosa

A Rust CLI tool that converts Excalidraw JSON diagrams to PNG images.

## Features

- Converts Excalidraw JSON files to PNG format
- Supports all major Excalidraw element types:
  - Rectangles with optional rounded corners
  - Diamonds
  - Ellipses/Circles
  - Lines and Arrows
  - Text elements
- Automatic canvas sizing with padding
- Customizable output path
- Comprehensive error handling

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

### Help

```bash
excaliosa --help
```

## Supported Element Types

- **Rectangle**: Renders as SVG rectangle with optional rounded corners
- **Diamond**: Renders as SVG polygon
- **Ellipse**: Renders as SVG ellipse
- **Circle**: Renders as SVG ellipse (circle is a special case of ellipse)
- **Arrow/Line**: Renders as SVG path with optional arrowhead marker
- **Text**: Renders as SVG text element with font size and color

## Element Properties

The converter handles the following Excalidraw element properties:

- **Geometry**: x, y, width, height, angle, points
- **Styling**: strokeColor, backgroundColor, opacity, strokeWidth
- **Shapes**: roundness for rounded rectangles
- **Special**: Text content, font size, arrow types

## SVG to PNG Conversion

The tool uses `resvg` for rendering SVG to PNG, which provides:

- Pure Rust implementation (no external dependencies)
- High-quality rendering
- White background by default
- Support for markers and advanced SVG features

## Example Workflow

```bash
# Create a diagram in Excalidraw
# Export it as JSON

# Convert to PNG
excaliosa my_diagram.json

# Or with custom output
excaliosa my_diagram.json -o diagrams/my_output.png
```

## Architecture

The project is organized into several modules:

- **models.rs**: Data structures mirroring Excalidraw JSON format
- **renderer.rs**: SVG generation from Excalidraw elements
- **converter.rs**: SVG to PNG conversion using resvg
- **main.rs**: CLI interface and argument parsing

## Error Handling

The tool provides comprehensive error messages:

- Invalid JSON files
- Missing input files
- File I/O errors
- SVG rendering errors
- PNG conversion errors

## Testing

A test diagram is included at `test_diagram.json`. To test:

```bash
cargo build --release
./target/release/excaliosa test_diagram.json
```

This will generate `test_diagram.png`.

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
