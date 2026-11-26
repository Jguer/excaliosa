//! Embedded font data and metadata shared by the SVG and raster backends.

pub struct BundledFont {
    pub bytes: &'static [u8],
    pub generic_family: &'static str,
    pub embed_in_svg: bool,
}

pub const EXCALIFONT_REGULAR: &[u8] = include_bytes!("../fonts/Excalifont-Regular.ttf");
pub const LIBERATION_SANS_REGULAR: &[u8] = include_bytes!("../fonts/LiberationSans-Regular.ttf");
pub const CASCADIA_CODE: &[u8] = include_bytes!("../fonts/CascadiaCode.ttf");
pub const COMIC_SHANNS_REGULAR: &[u8] = include_bytes!("../fonts/ComicShanns-Regular.ttf");

/// Deterministic PNG glyph fallback order; the primary face is searched first.
pub const BUNDLED_FONT_FALLBACK_ORDER: [&str; 4] = [
    "Cascadia Code",
    "Liberation Sans",
    "Comic Shanns",
    "Excalifont",
];

/// Look up the bundled substitute for a scene font family.
pub fn bundled_font(family: &str) -> BundledFont {
    match family {
        "Comic Shanns" => BundledFont {
            bytes: COMIC_SHANNS_REGULAR,
            generic_family: "monospace",
            // Installable embedding; copyright and complete MIT notice are in the name table.
            embed_in_svg: true,
        },
        "Liberation Sans" => BundledFont {
            bytes: LIBERATION_SANS_REGULAR,
            generic_family: "sans-serif",
            // The complete license is shipped separately in LiberationSans-LICENSE.txt.
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
