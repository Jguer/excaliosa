use anyhow::{Context, Result};
use clap::Parser;
use excaliosa::{convert_svg_to_png, generate_svg};
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "excaliosa")]
#[command(about = "Convert Excalidraw JSON to PNG", long_about = None)]
struct Args {
    /// Path to the Excalidraw JSON file
    #[arg(value_name = "FILE")]
    input: PathBuf,

    /// Output PNG file path (defaults to input filename with .png extension)
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Read the JSON file
    let json_content = fs::read_to_string(&args.input)
        .with_context(|| format!("Failed to read input file: {:?}", args.input))?;

    // Parse the JSON
    let excalidraw_data: excaliosa::ExcalidrawData = serde_json::from_str(&json_content)
        .context("Failed to parse Excalidraw JSON")?;

    // Generate SVG
    let svg_content = generate_svg(&excalidraw_data);

    // Determine output path
    let output_path = args.output.unwrap_or_else(|| {
        let mut path = args.input.clone();
        path.set_extension("png");
        path
    });

    // Convert SVG to PNG
    convert_svg_to_png(&svg_content, &output_path)
        .with_context(|| format!("Failed to convert to PNG: {:?}", output_path))?;

    println!(
        "Successfully converted {} to {}",
        args.input.display(),
        output_path.display()
    );

    Ok(())
}
