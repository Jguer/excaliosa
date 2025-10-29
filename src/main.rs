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
}

fn main() -> Result<()> {
    let args = Args::parse();

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
            let svg_content = generate_svg(&excalidraw_data);
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
                let svg_content = generate_svg(&excalidraw_data);
                convert_svg_to_png(&svg_content, &output_path)
                    .with_context(|| format!("Failed to convert to PNG: {output_path:?}"))?;
            } else {
                // Use rough_tiny_skia renderer (direct PNG output)
                render_to_png(&excalidraw_data, &output_path)
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
