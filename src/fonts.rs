//! Embedded font data and metadata shared by the SVG and raster backends.

pub struct BundledFont {
    pub bytes: &'static [u8],
    pub generic_family: &'static str,
    pub embed_in_svg: bool,
}

pub const EXCALIFONT_REGULAR: &[u8] = include_bytes!("../fonts/Excalifont-Regular.ttf");
pub const LIBERATION_SANS_REGULAR: &[u8] = include_bytes!("../fonts/LiberationSans-Regular.ttf");
pub const CASCADIA_CODE: &[u8] = include_bytes!("../fonts/CascadiaCode.ttf");

/// Look up the bundled substitute for a scene font family.
pub fn bundled_font(family: &str) -> BundledFont {
    match family {
        "Liberation Sans" => BundledFont {
            bytes: LIBERATION_SANS_REGULAR,
            generic_family: "sans-serif",
            // Its name table permits editable embedding but does not contain the license.
            embed_in_svg: false,
        },
        "Cascadia Code" => BundledFont {
            bytes: CASCADIA_CODE,
            generic_family: "monospace",
            // Installable embedding; copyright and complete OFL are in the name table.
            embed_in_svg: true,
        },
        _ => BundledFont {
            bytes: EXCALIFONT_REGULAR,
            generic_family: "sans-serif",
            // Editable embedding; copyright and complete OFL are in the name table.
            embed_in_svg: true,
        },
    }
}
