use anyhow::Result;
use resvg::usvg::{self, Tree};
use std::path::Path;
use tiny_skia::Pixmap;

// Include fonts as bytes
pub const EXCALIFONT_REGULAR: &[u8] = include_bytes!("../fonts/Excalifont-Regular.ttf");
pub const LIBERATION_SANS_REGULAR: &[u8] = include_bytes!("../fonts/LiberationSans-Regular.ttf");
pub const LIBERATION_SANS_BOLD: &[u8] = include_bytes!("../fonts/LiberationSans-Bold.ttf");
pub const CASCADIA_CODE: &[u8] = include_bytes!("../fonts/CascadiaCode.ttf");

pub fn convert_svg_to_png(svg_content: &str, output_path: &Path, background: Option<(u8,u8,u8,u8)>, quality: u8, dpi: Option<u32>) -> Result<()> {
    // Prepare usvg options and load embedded fonts into its font database
    let mut options = usvg::Options::default();
    // Build a font database and then assign it to options (options.fontdb is Arc)
    let mut fontdb = fontdb::Database::new();
    fontdb.load_font_data(EXCALIFONT_REGULAR.to_vec());
    fontdb.load_font_data(LIBERATION_SANS_REGULAR.to_vec());
    fontdb.load_font_data(LIBERATION_SANS_BOLD.to_vec());
    fontdb.load_font_data(CASCADIA_CODE.to_vec());
    options.fontdb = std::sync::Arc::new(fontdb);

    // Parse SVG
    let tree = Tree::from_str(svg_content, &options)?;

    // Calculate scale factor from DPI (assume source is 96 DPI)
    const SOURCE_DPI: f32 = 96.0;
    let scale = dpi.map(|d| d as f32 / SOURCE_DPI).unwrap_or(1.0);

    // Get dimensions from SVG viewBox or use default
    let size = tree.size();
    let width = (size.width() * scale).ceil() as u32;
    let height = (size.height() * scale).ceil() as u32;

    // Ensure minimum dimensions
    let width = width.max(100);
    let height = height.max(100);

    // Create pixmap
    let mut pixmap = Pixmap::new(width, height)
        .ok_or_else(|| anyhow::anyhow!("Failed to create pixmap"))?;

    // Fill with background (default white if None)
    if let Some((r,g,b,a)) = background.or(Some((255,255,255,255))) {
        if a > 0 {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(r,g,b,a);
            pixmap.fill_rect(
                tiny_skia::Rect::from_xywh(0.0, 0.0, width as f32, height as f32).unwrap(),
                &paint,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }

    // Render SVG to pixmap with scaling transform
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(
        &tree,
        transform,
        &mut pixmap.as_mut(),
    );

    // Save as PNG with quality control
    save_png_with_quality(&pixmap, output_path, quality)?;

    Ok(())
}

/// Save a pixmap to PNG with compression quality control (0-100).
/// Maps 0-100 to PNG compression types:
/// - 0-25: Fast (fastest encoding, larger files)
/// - 26-75: Default (balanced)
/// - 76-100: Best (slowest encoding, smallest files)
fn save_png_with_quality(
    pixmap: &tiny_skia::Pixmap,
    output_path: &std::path::Path,
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
