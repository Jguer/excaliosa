use anyhow::{Context, Result, ensure};
use clap::Parser;
use excaliosa::{
    color_utils::parse_color_result, generate_svg, render_to_png, utils::write_output,
};
use same_file::Handle;
use std::fs;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(name = "excaliosa")]
#[command(about = "Convert Excalidraw JSON to PNG or SVG", long_about = None)]
struct Args {
    /// Path to the Excalidraw JSON file
    #[arg(value_name = "FILE")]
    input: PathBuf,

    /// Output file path (defaults to input filename with .png extension)
    /// Use .svg extension to export as SVG, .png for PNG.
    /// On success the destination is replaced, not written through a link.
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Background color hex (e.g. #RRGGBB or #RRGGBBAA). Use "transparent" for full transparency
    #[arg(short = 'b', long = "background", value_name = "HEX")]
    background: Option<String>,

    /// PNG compression quality (0-100). Higher values produce smaller files but slower encoding.
    /// 0-25: Fast encoding, 26-75: Balanced, 76-100: Best compression.
    /// Only applies to PNG output. Default: 75
    #[arg(short = 'q', long = "quality", value_name = "0-100", default_value = "75", value_parser = clap::value_parser!(u8).range(0..=100))]
    quality: u8,

    /// Target DPI for output scaling. Assumes source is 96 DPI.
    /// Must be positive. Lower values reduce size (e.g., --dpi 48 halves dimensions).
    /// PNG output is limited to 8192 pixels per axis and 16,777,216 pixels total.
    /// Only applies to PNG output. Default: None (use original dimensions)
    #[arg(long = "dpi", value_name = "DPI", value_parser = clap::value_parser!(u32).range(1..))]
    dpi: Option<u32>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Parse optional background color
    let bg_rgba: Option<(u8, u8, u8, u8)> = args
        .background
        .as_deref()
        .map(|s| parse_color_result(s).map_err(|e| anyhow::anyhow!(e)))
        .transpose()
        .with_context(
            || "Invalid --background value. Use #RRGGBB or #RRGGBBAA or 'transparent'.",
        )?;

    // Determine output path
    let output_path = args.output.unwrap_or_else(|| {
        let mut path = args.input.clone();
        path.set_extension("png");
        path
    });

    let input = Handle::from_path(&args.input)
        .with_context(|| format!("Failed to open input file: {:?}", args.input))?;
    reject_input_alias(&input, &output_path)
        .with_context(|| format!("Invalid output path: {output_path:?}"))?;
    let mut json_content = String::new();
    input
        .as_file()
        .read_to_string(&mut json_content)
        .with_context(|| format!("Failed to read input file: {:?}", args.input))?;
    let excalidraw_data: excaliosa::ExcalidrawData =
        serde_json::from_str(&json_content).context("Failed to parse Excalidraw JSON")?;

    // Check if output is SVG or PNG based on extension
    let extension = output_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("png");

    match extension.to_lowercase().as_str() {
        "svg" => {
            // Generate SVG directly
            let svg_content = generate_svg(&excalidraw_data, bg_rgba);
            write_output(&output_path, |out| {
                Ok(out.write_all(svg_content.as_bytes())?)
            })
            .with_context(|| format!("Failed to write SVG file: {output_path:?}"))?;
        }
        _ => {
            // Rasterize directly to PNG via the Skia backend.
            render_to_png(
                &excalidraw_data,
                &output_path,
                bg_rgba,
                args.quality,
                args.dpi,
            )
            .with_context(|| format!("Failed to render PNG: {output_path:?}"))?;
        }
    }

    println!(
        "Successfully converted {} to {}",
        args.input.display(),
        output_path.display()
    );
    Ok(())
}

fn reject_input_alias(input: &Handle, output: &Path) -> Result<()> {
    match fs::metadata(output) {
        Ok(meta) => ensure!(meta.is_file(), "Output is not a regular file"),
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    }
    ensure!(&Handle::from_path(output)? != input, "Output aliases input");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_input_path_and_hardlink() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input_path = dir.path().join("drawing.json");
        let link = dir.path().join("drawing.png");
        fs::write(&input_path, "{}")?;
        let input = Handle::from_path(&input_path)?;
        assert!(reject_input_alias(&input, &input_path).is_err());
        reject_input_alias(&input, &link)?;
        fs::hard_link(&input_path, &link)?;
        assert!(reject_input_alias(&input, &link).is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_to_input() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input_path = dir.path().join("drawing.json");
        let link = dir.path().join("drawing.svg");
        fs::write(&input_path, "{}")?;
        std::os::unix::fs::symlink(&input_path, &link)?;
        assert!(reject_input_alias(&Handle::from_path(&input_path)?, &link).is_err());
        Ok(())
    }
}
