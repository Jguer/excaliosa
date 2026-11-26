use excaliosa::fonts::{
    BUNDLED_FONT_FALLBACK_ORDER, CASCADIA_CODE, COMIC_SHANNS_REGULAR, EXCALIFONT_REGULAR,
    bundled_font,
};
use excaliosa::scene::Primitive;
use excaliosa::{ExcalidrawData, build_scene, generate_svg, render_to_png};
use serde_json::{Value, json};
use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, GlyphId, MetadataProvider};

const FONT_SIZE: f64 = 48.0;
const TARGET_BASELINE: f64 = 64.0;
const MIXED: &str = "A→B✓C";

#[derive(Debug, PartialEq)]
struct Raster {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

fn document(elements: Vec<Value>) -> ExcalidrawData {
    serde_json::from_value(json!({"type": "excalidraw", "elements": elements})).unwrap()
}

fn text(
    id: String,
    content: String,
    family: i32,
    x: f64,
    width: f64,
    align: &str,
    line_height: f64,
) -> Value {
    json!({
        "id": id,
        "type": "text",
        "x": x,
        "y": 0,
        "width": width,
        "height": 100,
        "text": content,
        "fontFamily": family,
        "fontSize": FONT_SIZE,
        "lineHeight": line_height,
        "textAlign": align,
        "strokeColor": "#000000",
        "opacity": 100
    })
}

fn line_height_for_baseline(family: i32) -> f64 {
    let data = document(vec![text(
        "probe".into(),
        "A".into(),
        family,
        0.0,
        400.0,
        "left",
        1.0,
    )]);
    let scene = build_scene(&data, None);
    let Primitive::Text(text) = &scene.drawables[0].primitive else {
        panic!("probe did not produce text");
    };
    1.0 + (TARGET_BASELINE - text.baseline_offset) * 2.0 / FONT_SIZE
}

fn render_elements(mut elements: Vec<Value>) -> Raster {
    elements.insert(
        0,
        json!({
            "id": "fixed-viewbox",
            "type": "rectangle",
            "x": -40,
            "y": -40,
            "width": 480,
            "height": 180,
            "opacity": 0,
            "strokeColor": "transparent",
            "backgroundColor": "transparent"
        }),
    );
    let data = document(elements);
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
    Raster {
        width: frame.width,
        height: frame.height,
        pixels,
    }
}

fn render_single(content: &str, family: i32) -> Raster {
    render_elements(vec![text(
        "single".into(),
        content.into(),
        family,
        0.0,
        400.0,
        "left",
        line_height_for_baseline(family),
    )])
}

fn has_visible_pixels(raster: &Raster) -> bool {
    raster.pixels.as_chunks::<4>().0.iter().any(|px| px[3] != 0)
}

fn mapped_glyph(font: &FontRef, ch: char) -> Option<GlyphId> {
    font.charmap()
        .map(ch)
        .filter(|glyph| *glyph != GlyphId::NOTDEF)
}

fn advance(font: &FontRef, ch: char) -> f32 {
    let glyph = mapped_glyph(font, ch).unwrap();
    font.glyph_metrics(Size::new(FONT_SIZE as f32), LocationRef::default())
        .advance_width(glyph)
        .unwrap()
}

#[test]
fn bundled_charmaps_determine_fallback_and_notdef_cases() {
    let excalifont = FontRef::new(EXCALIFONT_REGULAR).unwrap();
    let cascadia = FontRef::new(CASCADIA_CODE).unwrap();
    assert!(mapped_glyph(&excalifont, 'A').is_some());
    for symbol in ['→', '✓'] {
        assert_eq!(mapped_glyph(&excalifont, symbol), None);
        assert!(mapped_glyph(&cascadia, symbol).is_some());
    }

    for missing in ['🦄', '🦓'] {
        for family in BUNDLED_FONT_FALLBACK_ORDER {
            let font = FontRef::new(bundled_font(family).bytes).unwrap();
            assert_eq!(mapped_glyph(&font, missing), None, "{family}: {missing}");
        }
    }
}

#[test]
fn fallback_symbols_use_cascadia_outlines_and_unsupported_scalars_match_notdef() {
    for symbol in ['→', '✓'] {
        assert_eq!(
            render_single(&symbol.to_string(), 5),
            render_single(&symbol.to_string(), 3)
        );
    }

    let primary_hit = render_single("A", 5);
    let missing = render_single("🦄", 5);
    assert_ne!(primary_hit, missing);
    assert!(has_visible_pixels(&primary_hit));
    assert!(has_visible_pixels(&missing));
    assert_eq!(missing, render_single("🦓", 5));
}

#[test]
fn primary_face_wins_for_shared_glyphs() {
    let comic = FontRef::new(COMIC_SHANNS_REGULAR).unwrap();
    let cascadia = FontRef::new(CASCADIA_CODE).unwrap();
    assert!(mapped_glyph(&comic, '→').is_some());
    assert!(mapped_glyph(&cascadia, '→').is_some());

    let primary = render_single("→", 2);
    assert!(has_visible_pixels(&primary));
    assert_ne!(primary, render_single("→", 3));
}

#[test]
fn mixed_runs_use_resolved_advances_for_every_alignment() {
    let primary = FontRef::new(EXCALIFONT_REGULAR).unwrap();
    let fallback = FontRef::new(CASCADIA_CODE).unwrap();
    let line_height = line_height_for_baseline(5);
    let line_width: f64 = MIXED
        .chars()
        .map(|ch| {
            let font = if mapped_glyph(&primary, ch).is_some() {
                &primary
            } else {
                &fallback
            };
            f64::from(advance(font, ch))
        })
        .sum();

    for align in ["left", "center", "right"] {
        let start_x = match align {
            "center" => (400.0 - line_width) / 2.0,
            "right" => 400.0 - line_width,
            _ => 0.0,
        };
        let actual = render_elements(vec![text(
            "mixed".into(),
            MIXED.into(),
            5,
            0.0,
            400.0,
            align,
            line_height,
        )]);

        let mut cursor_x = start_x;
        let mut expected_elements = Vec::new();
        for (index, ch) in MIXED.chars().enumerate() {
            expected_elements.push(text(
                format!("glyph-{index}"),
                ch.to_string(),
                5,
                cursor_x,
                1.0,
                "left",
                line_height,
            ));
            let font = if mapped_glyph(&primary, ch).is_some() {
                &primary
            } else {
                &fallback
            };
            cursor_x += f64::from(advance(font, ch));
        }
        assert_eq!(actual, render_elements(expected_elements), "{align}");
    }
}

#[test]
fn svg_keeps_the_primary_face_and_saved_text() {
    let data: ExcalidrawData =
        serde_json::from_str(include_str!("fixtures/png_glyph_fallback.json")).unwrap();
    let svg = generate_svg(&data, None);
    assert!(svg.contains("font-family=\"Excalifont, sans-serif\""));
    assert!(!svg.contains("Cascadia Code"));
    for saved in [
        "Left A→B ✓ C",
        "Center A→B ✓ C",
        "Right A→B ✓ C",
        "Unsupported 🦄",
    ] {
        assert!(svg.contains(saved));
    }
}
