use std::collections::BTreeSet;

use base64::Engine;
use excaliosa::font_utils::get_font_family;
use excaliosa::fonts::{COMIC_SHANNS_REGULAR, bundled_font};
use excaliosa::scene::Primitive;
use excaliosa::{ExcalidrawData, build_scene, generate_svg, render_to_png};
use serde_json::{Value, json};
use skrifa::raw::TableProvider;
use skrifa::string::StringId;
use skrifa::{FontRef, GlyphId, MetadataProvider};

const LATIN: &[(u32, u32)] = &[
    (0x20, 0x7e),
    (0xa1, 0xa6),
    (0xa8, 0xa8),
    (0xab, 0xac),
    (0xaf, 0xb1),
    (0xb4, 0xb4),
    (0xb8, 0xb8),
    (0xbb, 0xbc),
    (0xbf, 0xcf),
    (0xd1, 0xd7),
    (0xd9, 0xde),
    (0xe0, 0xef),
    (0xf1, 0xf7),
    (0xf9, 0xff),
    (0x131, 0x131),
    (0x152, 0x153),
    (0x2c6, 0x2c6),
    (0x2da, 0x2da),
    (0x2dc, 0x2dc),
    (0x2013, 0x2014),
    (0x2018, 0x201a),
    (0x201c, 0x201d),
    (0x2020, 0x2022),
    (0x2026, 0x2026),
    (0x2039, 0x203a),
    (0x2044, 0x2044),
    (0x20ac, 0x20ac),
    (0x2191, 0x2191),
    (0x2193, 0x2193),
    (0x2212, 0x2212),
];
const LATIN_EXTENDED: &[(u32, u32)] = &[
    (0x100, 0x10f),
    (0x112, 0x125),
    (0x128, 0x130),
    (0x134, 0x137),
    (0x139, 0x13c),
    (0x141, 0x148),
    (0x14c, 0x151),
    (0x154, 0x161),
    (0x164, 0x165),
    (0x168, 0x17f),
    (0x1bf, 0x1bf),
    (0x1f7, 0x1f7),
    (0x218, 0x21b),
    (0x237, 0x237),
    (0x1e80, 0x1e85),
    (0x1ef2, 0x1ef3),
    (0xa75b, 0xa75b),
];
const SYMBOLS: &[(u32, u32)] = &[
    (0x2c7, 0x2c7),
    (0x2d8, 0x2d9),
    (0x2db, 0x2db),
    (0x2dd, 0x2dd),
    (0x315, 0x315),
    (0x2190, 0x2190),
    (0x2192, 0x2192),
    (0x2200, 0x2200),
    (0x2203, 0x2204),
    (0x2264, 0x2265),
    (0xf6c3, 0xf6c3),
];
const GREEK: &[(u32, u32)] = &[(0x3bb, 0x3bb)];
const SHARDS: &[&[(u32, u32)]] = &[LATIN, LATIN_EXTENDED, SYMBOLS, GREEK];

fn fixture() -> ExcalidrawData {
    serde_json::from_str(include_str!("fixtures/comic_shanns.json")).unwrap()
}

fn document(elements: Value) -> ExcalidrawData {
    serde_json::from_value(json!({"type": "excalidraw", "elements": elements})).unwrap()
}

fn rendered_rgba(font_family: i32, text: &str, line_height: f64) -> Vec<u8> {
    let data = document(json!([{
        "type": "text",
        "x": 0,
        "y": 0,
        "width": 400,
        "height": 60,
        "text": text,
        "fontFamily": font_family,
        "fontSize": 48,
        "lineHeight": line_height,
        "strokeColor": "#000000"
    }]));
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("text.png");
    render_to_png(&data, &output, Some((0, 0, 0, 0)), 75, None).unwrap();
    let mut decoder = png::Decoder::new(std::fs::File::open(output).unwrap());
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(frame.color_type, png::ColorType::Rgba);
    pixels.truncate(frame.buffer_size());
    reader.finish().unwrap();
    pixels
}

#[test]
fn ttf_has_the_complete_declared_map_metrics_and_embedding_license() {
    let font = FontRef::new(COMIC_SHANNS_REGULAR).unwrap();
    let charmap = font.charmap();
    let expected: BTreeSet<u32> = SHARDS
        .iter()
        .flat_map(|shard| shard.iter().flat_map(|&(start, end)| start..=end))
        .collect();
    let actual: BTreeSet<u32> = charmap.mappings().map(|(codepoint, _)| codepoint).collect();
    assert_eq!(expected.len(), 333);
    assert_eq!(actual, expected);
    for codepoint in expected {
        assert_ne!(
            charmap.map(codepoint),
            Some(GlyphId::NOTDEF),
            "U+{codepoint:04X}"
        );
    }

    assert_eq!(font.head().unwrap().units_per_em(), 1000);
    assert_eq!(font.hhea().unwrap().ascender().to_i16(), 750);
    assert_eq!(font.hhea().unwrap().descender().to_i16(), -250);
    assert_eq!(font.os2().unwrap().fs_type(), 0);
    let copyright = font
        .localized_strings(StringId::COPYRIGHT_NOTICE)
        .english_or_first()
        .unwrap()
        .to_string();
    assert_eq!(
        copyright,
        include_str!("../fonts/ComicShanns-Regular-LICENSE.txt").trim_end()
    );
}

#[test]
fn id_8_uses_comic_shanns_metrics_and_preserves_explicit_line_height() {
    assert_eq!(get_font_family(Some(8)), "Comic Shanns");
    assert_eq!(bundled_font("Comic Shanns").generic_family, "monospace");
    let scene = build_scene(&fixture(), None);
    let texts: Vec<_> = scene
        .drawables
        .iter()
        .filter_map(|drawable| match &drawable.primitive {
            Primitive::Text(text) => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(texts[0].font_family, "Comic Shanns");
    assert_eq!(texts[0].line_height, 35.0);
    assert_eq!(texts[0].baseline_offset, 24.5);
    assert_eq!(texts[1].font_family, "Comic Shanns");
    assert_eq!(texts[1].line_height, 42.0);
    assert_eq!(texts[1].baseline_offset, 28.0);
    assert_eq!(texts[3].font_family, "Excalifont");
    assert_eq!(texts[4].font_family, "Cascadia Code");
    assert_eq!(texts[5].font_family, "Liberation Sans");
}

#[test]
fn svg_embeds_the_bundled_face_once_only_when_id_8_is_used() {
    let svg = generate_svg(&fixture(), None);
    assert_eq!(svg.matches("@font-face").count(), 3);
    assert_eq!(
        svg.matches("font-family=\"Comic Shanns, monospace\"")
            .count(),
        3
    );
    let prefix = "@font-face { font-family: \"Comic Shanns\"; src: url(\"data:font/ttf;base64,";
    assert_eq!(svg.matches(prefix).count(), 1);
    let payload = svg.split(prefix).nth(1).unwrap().split('"').next().unwrap();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .unwrap();
    assert_eq!(decoded, COMIC_SHANNS_REGULAR);
    assert_eq!(decoded, bundled_font("Comic Shanns").bytes);

    let other_face = document(json!([{
        "type": "text",
        "text": "Excalifont only",
        "fontFamily": 5,
        "width": 200,
        "height": 30
    }]));
    assert!(!generate_svg(&other_face, None).contains("Comic Shanns"));
    let no_text = document(json!([{"type": "rectangle", "width": 20, "height": 20}]));
    assert!(!generate_svg(&no_text, None).contains("Comic Shanns"));
}

#[test]
fn png_uses_comic_shanns_outlines_not_excalifont_or_notdef() {
    let comic = rendered_rgba(8, "Comic Shanns", 1.25);
    let excalifont = rendered_rgba(5, "Comic Shanns", 1.238);
    assert_ne!(comic, excalifont);
    assert!(comic.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0));

    let supported = rendered_rgba(8, "λ→", 1.25);
    let notdef = rendered_rgba(8, "☃☃", 1.25);
    assert_ne!(supported, notdef);
}
