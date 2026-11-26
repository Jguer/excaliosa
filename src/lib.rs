pub mod arrow_utils;
pub mod backend;
pub mod color_utils;
pub mod font_utils;
pub mod fonts;
pub mod math_utils;
pub mod models;
pub mod rect_utils;
pub mod scene;
pub mod stroke_utils;
pub mod utils;

pub use backend::skia::render_to_png;
pub use backend::svg::generate_svg;
pub use models::{ExcalidrawData, ExcalidrawElement};
pub use scene::build_scene;
pub use utils::calculate_viewbox;

#[cfg(test)]
mod tests;
