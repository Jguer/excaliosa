use anyhow::Result;
use resvg::usvg::{self, Tree};
use std::path::Path;
use tiny_skia::Pixmap;

// Include fonts as bytes
pub const EXCALIFONT_REGULAR: &[u8] = include_bytes!("../fonts/Excalifont-Regular.ttf");
pub const LIBERATION_SANS_REGULAR: &[u8] = include_bytes!("../fonts/LiberationSans-Regular.ttf");
pub const LIBERATION_SANS_BOLD: &[u8] = include_bytes!("../fonts/LiberationSans-Bold.ttf");
pub const CASCADIA_CODE: &[u8] = include_bytes!("../fonts/CascadiaCode.ttf");

pub fn convert_svg_to_png(svg_content: &str, output_path: &Path, background: Option<(u8,u8,u8,u8)>) -> Result<()> {
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

    // Get dimensions from SVG viewBox or use default
    let size = tree.size();
    let width = size.width().ceil() as u32;
    let height = size.height().ceil() as u32;

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

    // Render SVG to pixmap
    resvg::render(
        &tree,
        tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );

    // Save as PNG
    pixmap.save_png(output_path)?;

    Ok(())
}
