![Excaliosa](static/logo.png)

# Excaliosa

_It's ExcaliOsa not ExcalioSAR._

A Rust CLI tool that converts Excalidraw JSON diagrams to PNG or SVG format. 

The objective is not a one-to-one match with Excalidraw rendering, but just enough for it to be useful when automated. For example, I usually only use rounded rectangles and arrows.

## Installation

### From Source

```bash
git clone <repository>
cd excaliosa
cargo build --release
```

The binary will be available at `target/release/excaliosa`.

### Docker

You can use the prebuilt container image from the GitHub Container Registry:

```bash
docker run --rm \
  --user "$(id -u):$(id -g)" \
  -v "$PWD:/work" -w /work \
  ghcr.io/jguer/excaliosa:latest examples/arrows.json -o out.png
```

If you're on a SELinux enabled system, keep the volume flag relabel like `-v "$PWD:/work:z"`.

Examples:

```bash
# Convert a local JSON diagram to PNG (output next to the file)
docker run --rm -v "$PWD:/work" -w /work ghcr.io/jguer/excaliosa:latest examples/arrows.json

# Specify an explicit output path
docker run --rm -v "$PWD:/work" -w /work ghcr.io/jguer/excaliosa:latest examples/arrows.json -o out.png

# Export to SVG
docker run --rm -v "$PWD:/work" -w /work ghcr.io/jguer/excaliosa:latest examples/arrows.json -o arrows.svg
```

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
	- Both formats use the same scene for shape geometry and bundled font metrics.
	- PNG export fails if an image has missing, malformed, or unsupported data. Use base64-encoded, non-animated PNGs for embedded images.
	- A successful export replaces the destination entry instead of writing through a symbolic or hard link. The input file and links to it are not valid destinations.
- -b, --background <HEX>: Background color to apply to the canvas. Accepts `#RRGGBB` or `#RRGGBBAA` (alpha supported), or the literal `transparent`.
	- Examples: `--background "#ffffff"`, `--background "#121212cc"`, `--background transparent`.
	- Applies to both PNG and SVG export. For SVG, a full-size background `<rect>` is inserted with `fill-opacity` derived from alpha.
- -q, --quality <0-100>: PNG compression quality (0-100). Higher values produce smaller files but slower encoding.
	- 0-25: fast encoding (larger files)
	- 26-75: balanced (default: 75)
	- 76-100: best compression (slowest)
	- Only applies to PNG output.
- --dpi <DPI>: Positive integer DPI for PNG output scaling (assumes source is 96 DPI).
	- Lower values reduce output size (e.g. `--dpi 48` halves dimensions).
	- PNG output is limited to 8,192 pixels per axis and 16,777,216 pixels total.
- -h, --help: Show help and exit.

### Text and fonts

SVG exports embed Excalifont, Cascadia Code, and Comic Shanns once per used face, with sans-serif, monospace, and monospace fallbacks respectively. The complete TTF files are base64-encoded without subsetting, adding about 250 KiB for Excalifont, 959 KiB for Cascadia Code, and 66 KiB for Comic Shanns to the SVG. Liberation Sans is named with a sans-serif fallback but is not embedded: its bundled font metadata permits embedding but refers to an external license rather than containing the complete notice. Excalifont and Cascadia Code include their copyright and SIL Open Font License notices in the embedded font metadata. Comic Shanns includes its complete MIT notice, also stored in `fonts/ComicShanns-Regular-LICENSE.txt`, and permits installable embedding (`fsType = 0`).

The bundled Comic Shanns 1.3.0 face was generated offline from Excalidraw's [`ComicShanns-Regular.sfd`](https://github.com/excalidraw/excalidraw/blob/dabd83c4aa6b9e42d0dd0a42be62200eb8e3cce9/packages/excalidraw/fonts/ComicShanns/ComicShanns-Regular.sfd), whose SHA-256 is `ccb87f5dda14ae0800b57d1052e4b9faddda8b2d4a1aeb2e28e6cb11722539fe`. Its 333-character map exactly matches all four declared upstream WOFF2 ranges. The shipped TTF SHA-256 is `8f9b5f5250dd2bfa07476f8c81672cdd0977cb99df79225e0ea2e0ff7bd9aed5`. To reproduce the conversion, put that pinned SFD in the working directory and use FontForge 20230101 (Debian package `1:20230101~dfsg-4+b1`, sources from 2025-01-07):

```bash
fontforge -lang=ff -c 'Open("ComicShanns-Regular.sfd"); Generate("fonts/ComicShanns-Regular.ttf")'
```

Both exports preserve saved lines and explicit line heights; SVG also records left-to-right or right-to-left direction and logical text anchors. PNG uses only bundled fonts for simple per-character glyph outlining. It checks the selected face and then the other bundled faces before drawing the selected font's notdef glyph for unsupported printable characters. PNG does not perform complex text shaping, bidirectional reordering, emoji fallback, system font lookup, or SVG-style font fallback.

### More examples

```bash
# Default PNG next to input
excaliosa examples/arrows.json           # -> examples/arrows.png

# Explicit PNG path
excaliosa examples/arrows.json -o out.png

# Export SVG directly
excaliosa examples/arrows.json -o arrows.svg

# Set a custom background color (opaque white)
excaliosa examples/logo.json -b "#ffffff" -o logo_white.png

# Semi-transparent dark background (80% opacity)
excaliosa examples/logo.json -b "#121212cc" -o logo_dark80.png

# Fully transparent background for SVG or PNG (PNG will have transparent pixels)
excaliosa examples/logo.json -b transparent -o logo_transparent.png

# Control PNG compression quality (higher = smaller file, slower)
excaliosa examples/logo.json -q 90 -o logo_q90.png

# Scale output via DPI (source is assumed 96 DPI)
excaliosa examples/logo.json --dpi 144 -o logo_144dpi.png
```

### Help

```bash
excaliosa --help
```

## Static element approximations

- Sticky notes use deterministic seeded paper jitter, rounded or lifted corners, a simple edge shadow, and a UTC creation-date footer. Saved bound text keeps its stored layout; the footer omits the year on narrow notes and is omitted for invalid timestamps or notes smaller than 75 units.
- Iframes and web embeds export as styled rectangles with escaped, conservatively truncated single-line labels. They never load remote resources or emit `foreignObject`, `srcdoc`, or embed-specific clickable markup.

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
