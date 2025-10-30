use anyhow::{Context, Result};
use clap::Parser;
use excaliosa::{convert_svg_to_png, generate_svg, render_to_png};
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "excaliosa")]
#[command(about = "Convert Excalidraw JSON to PNG or SVG", long_about = None)]
struct Args {
    /// Path to the Excalidraw JSON file
    #[arg(value_name = "FILE")]
    input: PathBuf,

    /// Output file path (defaults to input filename with .png extension)
    /// Use .svg extension to export as SVG, .png for PNG
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Use legacy SVG renderer instead of rough_tiny_skia (default is rough_tiny_skia)
    #[arg(long)]
    legacy: bool,

    /// Background color hex (e.g. #RRGGBB or #RRGGBBAA). Use "transparent" for full transparency
    #[arg(short = 'b', long = "background", value_name = "HEX")] 
    background: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Parse optional background color
    let bg_rgba: Option<(u8, u8, u8, u8)> = args
        .background
        .as_deref()
        .map(parse_hex_rgba)
        .transpose()
        .with_context(|| "Invalid --background value. Use #RRGGBB or #RRGGBBAA or 'transparent'.")?;

    // Read the JSON file
    let json_content = fs::read_to_string(&args.input)
        .with_context(|| format!("Failed to read input file: {:?}", args.input))?;

    // Parse the JSON
    let excalidraw_data: excaliosa::ExcalidrawData = serde_json::from_str(&json_content)
        .context("Failed to parse Excalidraw JSON")?;

    // Determine output path
    let output_path = args.output.unwrap_or_else(|| {
        let mut path = args.input.clone();
        path.set_extension("png");
        path
    });

    // Check if output is SVG or PNG based on extension
    let extension = output_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("png");

    match extension.to_lowercase().as_str() {
        "svg" => {
            // Generate SVG directly
            let svg_content = generate_svg(&excalidraw_data, bg_rgba);
            fs::write(&output_path, svg_content)
                .with_context(|| format!("Failed to write SVG file: {output_path:?}"))?;
            
            println!(
                "Successfully converted {} to {}",
                args.input.display(),
                output_path.display()
            );
        }
        _ => {
            // Convert to PNG
            if args.legacy {
                // Legacy SVG + resvg approach
                // Avoid double background: rasterizer will fill background; keep SVG transparent
                let svg_content = generate_svg(&excalidraw_data, None);
                convert_svg_to_png(&svg_content, &output_path, bg_rgba)
                    .with_context(|| format!("Failed to convert to PNG: {output_path:?}"))?;
            } else {
                // Use rough_tiny_skia renderer (direct PNG output)
                render_to_png(&excalidraw_data, &output_path, bg_rgba)
                    .with_context(|| format!("Failed to render PNG: {output_path:?}"))?;
            }

            println!(
                "Successfully converted {} to {}",
                args.input.display(),
                output_path.display()
            );
        }
    }

    Ok(())
}

/// Parse a hex color string into RGBA.
/// Accepts: 
/// - "transparent" => (0,0,0,0)
/// - #RRGGBB or RRGGBB
/// - #RRGGBBAA or RRGGBBAA (AA is alpha)
fn parse_hex_rgba(s: &str) -> Result<(u8, u8, u8, u8)> {
    if s.eq_ignore_ascii_case("transparent") {
        return Ok((0, 0, 0, 0));
    }
    let s = s.trim();
    let hex = if let Some(rest) = s.strip_prefix('#') { rest } else { s };
    match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16)?;
            let g = u8::from_str_radix(&hex[2..4], 16)?;
            let b = u8::from_str_radix(&hex[4..6], 16)?;
            Ok((r, g, b, 255))
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16)?;
            let g = u8::from_str_radix(&hex[2..4], 16)?;
            let b = u8::from_str_radix(&hex[4..6], 16)?;
            let a = u8::from_str_radix(&hex[6..8], 16)?;
            Ok((r, g, b, a))
        }
        _ => anyhow::bail!("Expected 6 or 8 hex digits (RRGGBB or RRGGBBAA)"),
    }
}
