---
applyTo: '**'
---

# Excaliosa Project Memory

## Project Type
- Feature Implementation: Convert React/TypeScript Excalidraw viewer to Rust CLI
- Output: PNG files from Excalidraw JSON files
- Status: COMPLETED

## Tech Stack
- **Language**: Rust (2021 edition)
- **Project Type**: CLI Binary + Library
- **Key Dependencies**:
  - `serde` & `serde_json` - JSON parsing
  - `resvg` (0.40) - SVG rendering
  - `tiny-skia` (0.11) - Rasterization engine
  - `fontdb` (0.16) - Font database
  - `clap` (4.4) - CLI argument parsing
  - `anyhow` (1.0) - Error handling

## Core Implementation Summary

### Models (models.rs)
- ExcalidrawElement: All 40+ fields from Excalidraw JSON
- ExcalidrawData: Root container structure
- ViewBox: Calculated SVG boundaries
- Supporting types: Binding, RoundnessType, BoundElement

### Renderer (renderer.rs)
- `calculate_viewbox()`: Calculates optimal bounds with 40px padding, ignores deleted elements
- `generate_svg()`: Creates complete SVG with marker definitions
- Element rendering support:
  - rectangle (with optional rounded corners)
  - ellipse/circle
  - diamond (4-point polygon)
  - text (with font size and color)
  - line/arrow (with optional arrowhead markers)

### Converter (converter.rs)
- `convert_svg_to_png()`: Uses resvg for high-quality rendering
- Handles font database initialization
- Creates white background pixmap
- PNG output at calculated optimal dimensions

### CLI (main.rs)
- clap-based argument parsing
- Input file path (required)
- Output path option (-o, --output)
- Comprehensive error messages with context

## Key Features Implemented

1. ✅ JSON parsing with serde
2. ✅ SVG generation from all Excalidraw element types
3. ✅ Automatic viewBox calculation with smart padding
4. ✅ SVG to PNG conversion using pure Rust (resvg)
5. ✅ CLI with help and custom output path
6. ✅ Comprehensive error handling
7. ✅ 8 unit tests (all passing)
8. ✅ Test diagrams (simple and complex)
9. ✅ Documentation (README.md, DEVELOPMENT.md)
10. ✅ Makefile with convenient commands

## Testing Results
- All 8 unit tests PASS
- CLI tests with two sample diagrams PASS
- Error handling tests (invalid JSON, missing files) PASS
- Custom output path option PASS
- Help command PASS

## Project Structure
```
excaliosa/
├── src/
│   ├── main.rs           # CLI entry point
│   ├── lib.rs            # Library exports
│   ├── models.rs         # Excalidraw data structures
│   ├── renderer.rs       # SVG generation
│   ├── converter.rs      # SVG to PNG
│   └── tests.rs          # 8 unit tests
├── Cargo.toml            # Dependencies
├── Makefile              # Development commands
├── README.md             # User guide
├── DEVELOPMENT.md        # Developer guide
├── test_diagram.json     # Simple test case
└── test_diagram_complex.json  # Complex test case
```

## Usage Examples

```bash
# Basic usage
excaliosa diagram.json

# Custom output
excaliosa diagram.json -o output.png

# Help
excaliosa --help
```

## Build & Test Commands
- `make build` - Debug build
- `make release` - Release build
- `make test` - Run tests
- `make test-cli` - Full CLI test suite
- `make clean` - Clean artifacts
- `make fmt` - Format code
- `make lint` - Run clippy

## Future Enhancement Ideas
1. Batch processing mode
2. SVG-only export (without PNG)
3. Custom theme/styling
4. Performance optimizations
5. Web UI (Tauri)

## Bug Fixes Applied

### Fix 1: Roundness Type Parsing
- **Issue**: Real Excalidraw JSON has `roundness.type` as integer (e.g., 2, 3) instead of string
- **Solution**: Changed `RoundnessType.roundness_type` from `String` to `serde_json::Value`
- **Result**: Now accepts both string and integer values

### Fix 2: Optional versionNonce Fields
- **Issue**: Real Excalidraw JSON files may not include `versionNonce` in root object
- **Solution**: Made both `ExcalidrawData.versionNonce` and `ExcalidrawElement.versionNonce` optional (Option<i32>)
- **Result**: Successfully parses real-world Excalidraw exports

### Fix 3: Additional Element Fields
- **Added Optional Fields**: 
  - `startArrowhead`, `endArrowhead` (alternative arrow types)
  - `lastCommittedPoint` (for path elements)
  - `elbowed` (for arrow connections)
  - `version` (element version number)
- **Result**: Full compatibility with current Excalidraw format

### Testing Status After Fixes
- ✅ All 8 unit tests pass
- ✅ test_diagram.json converts successfully
- ✅ test_diagram_complex.json converts successfully
- ✅ 2025-08-how-to-grafana-access.excalidraw.json (4522 lines) converts successfully
- ✅ Generates 2718x2446 PNG from real Grafana diagram

## Notes
- Binary size: ~1.5MB (release build)
- No system dependencies (pure Rust)
- Cross-platform (macOS, Linux, Windows)
- Pure Rust rendering (no libav or system graphics libraries needed)
- White background by default (can be customized)
- Handles all major Excalidraw element types
- Full error context and user-friendly messages

