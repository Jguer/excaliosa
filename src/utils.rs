use anyhow::Result;
use std::path::Path;
use tiny_skia::Pixmap;

/// Save a pixmap to PNG with compression quality control (0-100).
/// Maps 0-100 to PNG compression types:
/// - 0-25: Fast (fastest encoding, larger files)
/// - 26-75: Default (balanced)
/// - 76-100: Best (slowest encoding, smallest files)
pub fn save_png_with_quality(
    pixmap: &Pixmap,
    output_path: &Path,
    quality: u8,
) -> Result<()> {
    use std::io::BufWriter;
    use std::fs::File;
    
    let file = File::create(output_path)
        .map_err(|e| anyhow::anyhow!("Failed to create PNG file: {e}"))?;
    let writer = BufWriter::new(file);
    
    let mut encoder = png::Encoder::new(writer, pixmap.width(), pixmap.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_filter(png::FilterType::Paeth);
    
    // Map quality 0-100 to compression type
    let compression_type = if quality <= 25 {
        png::Compression::Fast
    } else if quality <= 75 {
        png::Compression::Default
    } else {
        png::Compression::Best
    };
    encoder.set_compression(compression_type);
    
    let mut writer = encoder.write_header()
        .map_err(|e| anyhow::anyhow!("Failed to write PNG header: {e}"))?;
    
    // Write RGBA data
    let data = pixmap.data();
    writer.write_image_data(data)
        .map_err(|e| anyhow::anyhow!("Failed to write PNG data: {e}"))?;
    
    Ok(())
}

