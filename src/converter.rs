use anyhow::Result;
use resvg::usvg::{self, Tree};
use std::path::Path;
use tiny_skia::Pixmap;

// Include fonts as bytes
pub const EXCALIFONT_REGULAR: &[u8] = include_bytes!("../fonts/Excalifont-Regular.ttf");
pub const LIBERATION_SANS_REGULAR: &[u8] = include_bytes!("../fonts/LiberationSans-Regular.ttf");
pub const LIBERATION_SANS_BOLD: &[u8] = include_bytes!("../fonts/LiberationSans-Bold.ttf");
pub const CASCADIA_CODE: &[u8] = include_bytes!("../fonts/CascadiaCode.ttf");

pub fn convert_svg_to_png(svg_content: &str, output_path: &Path) -> Result<()> {
    // Create font database and load embedded fonts
    let mut fontdb = fontdb::Database::new();
    fontdb.load_font_data(EXCALIFONT_REGULAR.to_vec());
    fontdb.load_font_data(LIBERATION_SANS_REGULAR.to_vec());
    fontdb.load_font_data(LIBERATION_SANS_BOLD.to_vec());
    fontdb.load_font_data(CASCADIA_CODE.to_vec());

    // Parse SVG
    let tree = Tree::from_str(svg_content, &usvg::Options::default(), &fontdb)?;

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

    // Fill with white background
    pixmap.fill(tiny_skia::Color::WHITE);

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
