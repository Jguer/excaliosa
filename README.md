<div align="center">
  <img src="static/logo.png" alt="Excaliosa logo" width="360" />
  <h1>Excaliosa</h1>
  <p><strong>Convert Excalidraw JSON diagrams to PNG or SVG from the command line.</strong></p>
  <p><em>It's ExcaliOsa not ExcalioSAR.</em></p>
  <p>
    <a href="https://github.com/Jguer/excaliosa/actions/workflows/rust.yml"><img alt="CI" src="https://github.com/Jguer/excaliosa/actions/workflows/rust.yml/badge.svg" /></a>
    <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg" /></a>
    <a href="https://ghcr.io/jguer/excaliosa"><img alt="GHCR container" src="https://img.shields.io/badge/GHCR-excaliosa-2496ED?logo=docker&amp;logoColor=white" /></a>
    <a href="Cargo.toml"><img alt="Rust edition 2024" src="https://img.shields.io/badge/Rust-2024%20edition-000000?logo=rust" /></a>
  </p>
</div>

Excaliosa is a Rust CLI for turning Excalidraw JSON into PNG or SVG without a browser or JavaScript runtime. Both backends share scene geometry and bundled font metrics, making the tool suitable for repeatable exports in scripts and CI. It aims to be useful for automation rather than to reproduce every Excalidraw rendering detail pixel for pixel.

## Table of contents

- [Features](#features)
- [Installation](#installation)
- [Usage](#usage)
- [Output formats](#output-formats)
- [Fonts](#fonts)
- [Known limitations and differences from Excalidraw](#known-limitations-and-differences-from-excalidraw)
- [Development](#development)
- [Contributing](#contributing)
- [License](#license)

## Features

| Area | Support |
| --- | --- |
| Rectangles, diamonds, and ellipses | Rough geometry, rounded rectangles and diamonds, rotation, opacity, solid/dashed/dotted strokes, and solid, hachure, cross-hatch, or zigzag fills. Bound text is retained for rectangles, diamonds, and ellipses. |
| Lines and arrows | Straight multi-point paths, curved paths, rounded elbow arrows, and closed-line fills. Caps follow the generated shaft, including curved and elbow geometry. |
| Arrowheads | `arrow`, `bar`, `circle`, `circle_outline`, `triangle`, `triangle_outline`, `diamond`, and `diamond_outline`, with legacy `dot` support. |
| Cardinality markers | `cardinality_one`, `cardinality_many`, `cardinality_one_or_many`, `cardinality_exactly_one`, `cardinality_zero_or_one`, and `cardinality_zero_or_many`, plus supported legacy crowfoot names. |
| Arrow labels | Saved or fractional label positions on straight, curved, and elbow arrows. A padded mask keeps the arrow shaft out of the label area. |
| Freedraw | Constant- and variable-width ink, recorded or simulated pressure, streamline smoothing, dots, reversals, and closed-loop fills. The outline algorithm is an approximation. |
| Text | Bundled fonts, multiline text, explicit line heights, left/center/right alignment, and preserved whitespace and logical text direction in SVG. |
| Frames | Frame and Magicframe names and outlines, inherited opacity, and clipping for contained non-arrow content. |
| Sticky notes | Seeded paper jitter, rounded or lifted corners, an edge shadow, saved bound text, and a deterministic UTC creation-date footer when space permits. |
| Embed placeholders | Static styled rectangles for `iframe` and `embeddable` elements, with escaped, bounded single-line labels. Remote and active content is never loaded. |
| Images | Embedded raster data URLs with rotation, horizontal/vertical flipping, validated cropping, rounded corners, and shared assets for repeated images. Format support differs by backend; see [Output formats](#output-formats). |
| SVG links | Validated HTTP(S), `mailto:`, and same-document fragment links around visible SVG elements. Unsafe or malformed links are omitted. |
| Named colors | Case-insensitive CSS/SVG named colors in scene paints, along with hexadecimal colors and alpha. The CLI background option intentionally accepts only full hexadecimal values or `transparent`. |
| Legacy-file import | Legacy `draw` elements, incomplete linear point data, legacy text font shorthand and line heights, legacy arrowhead fields, and omitted paint defaults. |

## Installation

### Build from source

A Rust toolchain with Rust 2024 edition support is required. `Cargo.toml` selects edition 2024 and does not declare a separate `rust-version`/MSRV.

```bash
git clone https://github.com/Jguer/excaliosa.git
cd excaliosa
cargo build --release
```

The binary is written to `target/release/excaliosa`. To install it into Cargo's binary directory instead:

```bash
cargo install --path .
```

### Docker and GHCR

The container entrypoint is `excaliosa`, so pass the input and options directly to `docker run`. Mount the working directory to read examples and write exports:

```bash
docker run --rm \
  --user "$(id -u):$(id -g)" \
  -v "$PWD:/work" -w /work \
  ghcr.io/jguer/excaliosa:latest \
  examples/arrows.json -o arrows.png
```

Export SVG with the same image:

```bash
docker run --rm \
  --user "$(id -u):$(id -g)" \
  -v "$PWD:/work" -w /work \
  ghcr.io/jguer/excaliosa:latest \
  examples/arrows.json -o arrows.svg
```

On SELinux systems, add a volume relabel flag, for example `-v "$PWD:/work:z"`.

The `Dockerfile` builds the Rust binary in Alpine and runs it as an unprivileged user. The Docker workflow publishes `linux/amd64` and `linux/arm64` images for version tags or manual runs after the Rust workflow passes.

## Usage

```text
excaliosa [OPTIONS] <FILE>
```

The output format is selected from the output extension: `.svg` (case-insensitive) selects SVG; every other extension selects PNG.

```bash
# Default: write PNG beside the input as examples/arrows.png
excaliosa examples/arrows.json

# Choose an explicit PNG output path
excaliosa examples/arrows.json --output out.png

# Export SVG
excaliosa examples/arrows.json -o arrows.svg

# Apply an opaque background
excaliosa examples/logo.json --background "#ffffff" -o logo-white.png

# Apply an 80%-opaque background (RRGGBBAA)
excaliosa examples/logo.json -b "#121212cc" -o logo-dark.png

# Use a transparent canvas in either format
excaliosa examples/logo.json -b transparent -o logo-transparent.png

# Scale PNG dimensions from the assumed 96 DPI source (1.5x here)
excaliosa examples/logo.json --dpi 144 -o logo-144dpi.png

# Use slower PNG encoding with better compression
excaliosa examples/logo.json --quality 90 -o logo-compressed.png
```

| Option | Default | Description |
| --- | --- | --- |
| `<FILE>` | Required | Path to an Excalidraw JSON file. |
| `-o, --output <FILE>` | Input path with `.png` extension | Destination path. `.svg` selects SVG; all other extensions select PNG. |
| `-b, --background <HEX>` | Format-specific; see below | `#RRGGBB`, `#RRGGBBAA`, or `transparent`. Alpha is supported in both formats. |
| `-q, --quality <0-100>` | `75` | PNG compression only: 0-25 fast, 26-75 balanced, 76-100 best. Higher values favor smaller files over encoding speed. |
| `--dpi <DPI>` | Unscaled | Positive PNG DPI, assuming 96 DPI source geometry. `--dpi 48` halves dimensions; SVG ignores this option. |
| `-h, --help` | | Print CLI help. |

Existing destinations are replaced without prompting, but only after a successful render and encode. A temporary file is created beside the destination; an output symlink is replaced rather than followed. The input file and symbolic or hard links to it cannot be used as the destination.

<details>
<summary>Full CLI help</summary>

```text
Convert Excalidraw JSON to PNG or SVG

Usage: excaliosa [OPTIONS] <FILE>

Arguments:
  <FILE>  Path to the Excalidraw JSON file

Options:
  -o, --output <FILE>     Output file path (defaults to input filename with .png extension) Use .svg extension to export as SVG, .png for PNG. On success the destination is replaced, not written through a link
  -b, --background <HEX>  Background color hex (e.g. #RRGGBB or #RRGGBBAA). Use "transparent" for full transparency
  -q, --quality <0-100>   PNG compression quality (0-100). Higher values produce smaller files but slower encoding. 0-25: Fast encoding, 26-75: Balanced, 76-100: Best compression. Only applies to PNG output. Default: 75 [default: 75]
      --dpi <DPI>         Target DPI for output scaling. Assumes source is 96 DPI. Must be positive. Lower values reduce size (e.g., --dpi 48 halves dimensions). PNG output is limited to 8192 pixels per axis and 16,777,216 pixels total. Only applies to PNG output. Default: None (use original dimensions)
  -h, --help              Print help
```

</details>

## Output formats

Both formats consume the same lowered scene, so shape geometry, arrowheads, label placement, frame behavior, colors, and bundled font metrics are resolved once. Text and image capabilities still differ by backend.

| Behavior | SVG | PNG |
| --- | --- | --- |
| Artwork | Vector paths and live `<text>` elements. | Rasterized with `tiny-skia`; text is drawn from bundled glyph outlines. |
| Default canvas | Transparent unless `--background` is supplied. | Opaque white unless `--background` is supplied. |
| Custom background | Full-size rectangle with `fill-opacity` for alpha; no rectangle for full transparency. | Canvas fill with alpha; full transparency produces transparent pixels. |
| Embedded fonts | Excalifont, Cascadia Code, and Comic Shanns are embedded once per used family as complete base64 TTFs. Liberation Sans is named but not embedded. | No external font lookup or font embedding is needed; glyphs are rasterized from the bundled fonts. |
| Text behavior | Preserves whitespace, line breaks, alignment, direction, and logical anchors. Complex shaping is left to the SVG viewer. | Simple per-character layout with bundled-font fallback; no complex shaping or bidirectional reordering. |
| Images | Accepts embedded PNG, JPEG/JPG, GIF, and WebP data URLs; SVG data URLs are rejected. | Requires base64-encoded, static PNG data. Missing, malformed, unsupported, and animated images fail the export. |
| Links | Safe saved links become SVG anchors. | Link metadata does not affect pixels. |
| Scaling | ViewBox in scene units. `--dpi` and `--quality` do not apply. | `--dpi` scales from 96 DPI; `--quality` controls compression. |

PNG output and each embedded PNG are limited to **8,192 pixels per axis** and **16,777,216 pixels total**. Embedded PNGs also have a 24 MiB encoded-payload limit and an 8 MiB best-effort decoder workspace budget. These checks reject oversized images before allocating the full output buffer.

## Fonts

Excalidraw numeric font IDs map to bundled substitutes as follows:

| Excalidraw ID | Source face | Bundled family | Embedded in SVG | License and notice |
| --- | --- | --- | --- | --- |
| `1`, `5`, or unavailable/unknown IDs | Virgil, Excalifont, or an unavailable face | Excalifont | Yes, with `sans-serif` fallback | SIL Open Font License 1.1; copyright and complete notice are in the font metadata. |
| `2`, `9` | Helvetica or Liberation Sans | Liberation Sans | No; uses the named family with `sans-serif` fallback | Font metadata permits embedding but refers to an external Ascender license rather than containing the complete notice. |
| `3` | Cascadia | Cascadia Code | Yes, with `monospace` fallback | SIL Open Font License; copyright and complete notice are in the font metadata. |
| `8` | Comic Shanns | Comic Shanns | Yes, with `monospace` fallback | MIT; the complete notice is in the font metadata and [`fonts/ComicShanns-Regular-LICENSE.txt`](fonts/ComicShanns-Regular-LICENSE.txt). |

SVG embeds complete TTF files without subsetting, and only for families actually used by the scene. PNG checks the selected face first and then the other bundled faces for missing printable glyphs; unsupported characters use the selected font's notdef glyph. PNG never queries system fonts.

## Known limitations and differences from Excalidraw

| Area | Difference |
| --- | --- |
| Canvas padding | Excaliosa adds **40 scene units on each side** of the computed bounds; Excalidraw's standard export padding is 10. |
| Export controls | There is no dark-mode switch, general export-scale option, or frame-only export. `--dpi` scales PNG output only. |
| Legacy fonts | Virgil is rendered with Excalifont. Helvetica maps to Liberation Sans, which is not embedded in SVG and may fall back to a viewer's sans-serif font. |
| PNG text | There is no complex text shaping, bidirectional reordering, emoji fallback, or system-font lookup. SVG records text direction but relies on the viewer for shaping. |
| Freedraw | Speed-based simulated pressure, exponential streamline smoothing, offset outlines, and polygonal round caps approximate Excalidraw's freehand rendering rather than guaranteeing pixel parity. |
| Embeds and remote content | Iframes and web embeds are placeholders only. Remote resources, active embed content, `foreignObject`, and `srcdoc` are never loaded or emitted. |
| Unsupported elements | Unsupported element types are skipped and reported as a count on standard error. |

## Development

### Make targets

| Target | Command or purpose |
| --- | --- |
| `make help` | List available targets; this is the default target. |
| `make build` | `cargo build` in debug mode. |
| `make release` | `cargo build --release`. |
| `make test` | `cargo test`. |
| `make test-cli` | Build release and smoke-test default PNG, explicit PNG, and SVG conversion with sample diagrams. |
| `make clean` | `cargo clean`. |
| `make install` | Build release and copy the binary to `~/.cargo/bin/`. |
| `make fmt` | Format with `cargo fmt`. |
| `make lint` | Run `cargo clippy -- -D warnings`. |

### Tests and fixtures

```bash
cargo test
make test-cli
cargo fmt --check
cargo clippy -- -D warnings
```

Unit and integration tests cover import compatibility, geometry, arrowheads and cardinality, labels, frames, freedraw, static elements, image cropping and reuse, named colors, SVG links, fonts, and output safety. Focused Excalidraw JSON documents live in [`tests/fixtures/`](tests/fixtures/); additional sample diagrams are in [`examples/`](examples/).

The Rust workflow builds, runs all tests, and runs the CLI smoke test for pushes to `main` and pull requests. Keep changes formatted and warning-free with `make fmt` and `make lint`.

## Contributing

Issues and pull requests are welcome. For a focused contribution:

1. Open an issue for a bug or rendering difference, including a minimal Excalidraw JSON fixture when possible.
2. Add or update tests and fixtures for behavior changes.
3. Run the test, CLI smoke, formatting, and lint checks above.
4. Update this README when CLI options, supported elements, or documented limitations change.

## License

Excaliosa is available under the [MIT License](LICENSE). Bundled fonts retain their own licenses and notices; see [Fonts](#fonts) and the font metadata for details.
