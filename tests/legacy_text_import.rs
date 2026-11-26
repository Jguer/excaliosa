use excaliosa::models::ExcalidrawElement;
use excaliosa::scene::{Primitive, TextPrimitive};
use excaliosa::{ExcalidrawData, build_scene, generate_svg, render_to_png};
use serde_json::json;
use std::error::Error;
use std::path::Path;

fn fixture() -> ExcalidrawData {
    serde_json::from_str(include_str!("fixtures/legacy_text_import.json")).unwrap()
}

fn element<'a>(data: &'a ExcalidrawData, id: &str) -> &'a ExcalidrawElement {
    data.elements
        .iter()
        .find(|element| element.id == id)
        .unwrap()
}

fn text_primitive(data: &ExcalidrawData, id: &str) -> TextPrimitive {
    let content = element(data, id).text.as_deref().unwrap();
    build_scene(data, None)
        .drawables
        .into_iter()
        .find_map(|drawable| match drawable.primitive {
            Primitive::Text(text) if text.content == content => Some(text),
            _ => None,
        })
        .unwrap()
}

fn single_element_document(element: serde_json::Value) -> ExcalidrawData {
    serde_json::from_value(json!({"type": "excalidraw", "elements": [element]})).unwrap()
}

fn decoded_png(path: &Path) -> Result<(u32, u32, Vec<u8>), Box<dyn Error>> {
    let mut decoder = png::Decoder::new(std::fs::File::open(path)?);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut pixels)?;
    assert_eq!(frame.color_type, png::ColorType::Rgba);
    pixels.truncate(frame.buffer_size());
    reader.finish()?;
    Ok((frame.width, frame.height, pixels))
}

#[test]
fn legacy_multiline_uses_saved_metrics_and_normalized_line_count() {
    let data = fixture();
    let legacy = element(&data, "legacy-multiline");
    assert_eq!(legacy.font_size, Some(28.0));
    assert_eq!(legacy.font_family, Some(3));
    assert_eq!(legacy.line_height, Some(1.05));
    assert_eq!(legacy.text.as_deref(), Some("Legacy first\r\n\r\nthird\n"));

    let text = text_primitive(&data, "legacy-multiline");
    assert_eq!(text.font_family, "Cascadia Code");
    assert_eq!(text.font_size, 28.0);
    assert!((text.line_height - 29.4).abs() < 1e-10);
    assert!((text.baseline_offset - 24.40703125).abs() < 1e-10);
}

#[test]
fn legacy_and_explicitly_migrated_modern_exports_match() -> Result<(), Box<dyn Error>> {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/legacy_text_import.json"))?;
    let legacy_value = fixture["elements"][0].clone();
    let legacy = single_element_document(legacy_value.clone());
    let mut modern_value = legacy_value;
    let modern_object = modern_value.as_object_mut().unwrap();
    modern_object.remove("font");
    modern_object.insert("fontSize".into(), json!(28));
    modern_object.insert("fontFamily".into(), json!(3));
    modern_object.insert("lineHeight".into(), json!(1.05));
    let modern = single_element_document(modern_value);

    let serialized = serde_json::to_value(element(&legacy, "legacy-multiline"))?;
    assert_eq!(serialized.get("font"), None);
    assert_eq!(serialized.get("fontSize"), Some(&json!(28.0)));
    assert_eq!(serialized.get("fontFamily"), Some(&json!(3)));
    assert_eq!(serialized.get("lineHeight"), Some(&json!(1.05)));

    let legacy_text = text_primitive(&legacy, "legacy-multiline");
    let modern_text = text_primitive(&modern, "legacy-multiline");
    assert_eq!(legacy_text.content, modern_text.content);
    assert_eq!(legacy_text.font_family, modern_text.font_family);
    assert_eq!(legacy_text.font_size, modern_text.font_size);
    assert_eq!(legacy_text.line_height, modern_text.line_height);
    assert_eq!(legacy_text.baseline_offset, modern_text.baseline_offset);
    assert_eq!(generate_svg(&legacy, None), generate_svg(&modern, None));

    let directory = tempfile::tempdir()?;
    let legacy_path = directory.path().join("legacy.png");
    let modern_path = directory.path().join("modern.png");
    render_to_png(&legacy, &legacy_path, None, 75, None)?;
    render_to_png(&modern, &modern_path, None, 75, None)?;
    assert_eq!(decoded_png(&legacy_path)?, decoded_png(&modern_path)?);
    Ok(())
}

#[test]
fn explicit_advance_wins_and_modern_default_is_not_detected() {
    let data = fixture();
    assert_eq!(element(&data, "explicit-override").line_height, Some(1.6));
    let explicit = text_primitive(&data, "explicit-override");
    assert_eq!(explicit.font_family, "Liberation Sans");
    assert_eq!(explicit.line_height, 32.0);
    assert!((explicit.baseline_offset - 22.93359375).abs() < 1e-10);

    let modern = element(&data, "modern-default");
    assert_eq!(modern.line_height, None);
    let modern_text = text_primitive(&data, "modern-default");
    assert_eq!(modern_text.font_family, "Cascadia Code");
    assert_eq!(modern_text.font_size, 20.0);
    assert_eq!(modern_text.line_height, 24.0);

    let zero_override: ExcalidrawData = serde_json::from_value(json!({
        "type": "excalidraw",
        "elements": [{
            "id": "zero-override",
            "type": "text",
            "text": "two\nlines",
            "width": 100,
            "height": 80,
            "font": "20px Cascadia",
            "lineHeight": 0
        }]
    }))
    .unwrap();
    assert_eq!(
        element(&zero_override, "zero-override").line_height,
        Some(2.0)
    );
    assert_eq!(
        text_primitive(&zero_override, "zero-override").line_height,
        40.0
    );
}

#[test]
fn unusable_legacy_sizes_keep_valid_modern_settings() {
    for font in [
        "-12px Cascadia",
        "0px Cascadia",
        "1e-50px Cascadia",
        "1e100px Cascadia",
    ] {
        let data: ExcalidrawData = serde_json::from_value(json!({
            "type": "excalidraw",
            "elements": [{
                "id": "invalid-size",
                "type": "text",
                "text": "Modern settings",
                "width": 100,
                "height": 28,
                "font": font,
                "fontSize": 20,
                "fontFamily": 3,
                "lineHeight": 1.4
            }]
        }))
        .unwrap();
        let text = element(&data, "invalid-size");
        assert_eq!(text.font_size, Some(20.0), "font: {font}");
        assert_eq!(text.font_family, Some(3), "font: {font}");
        assert_eq!(text.line_height, Some(1.4), "font: {font}");
    }
}

#[test]
fn out_of_range_non_string_legacy_font_is_ignored() {
    let data: ExcalidrawData = serde_json::from_str(
        r#"{
            "type": "excalidraw",
            "elements": [{
                "id": "out-of-range-font",
                "type": "text",
                "text": "Modern settings",
                "width": 100,
                "height": 28,
                "font": 1e400,
                "fontSize": 20,
                "fontFamily": 3,
                "lineHeight": 1.4
            }]
        }"#,
    )
    .unwrap();
    let text = element(&data, "out-of-range-font");
    assert_eq!(text.font, None);
    assert_eq!(text.font_size, Some(20.0));
    assert_eq!(text.font_family, Some(3));
    assert_eq!(text.line_height, Some(1.4));
}

#[test]
fn fallback_and_malformed_records_keep_valid_modern_settings() {
    let data = fixture();
    let unknown = text_primitive(&data, "unknown-family");
    assert_eq!(element(&data, "unknown-family").font_family, Some(5));
    assert_eq!(unknown.font_family, "Excalifont");
    assert_eq!(unknown.font_size, 24.0);
    assert_eq!(unknown.line_height, 60.0);

    for id in ["missing-height", "zero-height"] {
        assert_eq!(element(&data, id).line_height, None);
        let text = text_primitive(&data, id);
        assert_eq!(text.font_family, "Cascadia Code");
        assert_eq!(text.line_height, 33.6);
    }

    let malformed = element(&data, "malformed-string");
    assert_eq!(malformed.font_size, Some(22.0));
    assert_eq!(malformed.font_family, Some(8));
    assert_eq!(malformed.line_height, None);
    assert_eq!(text_primitive(&data, "malformed-string").line_height, 27.5);

    let malformed_type = element(&data, "malformed-type");
    assert_eq!(malformed_type.font, None);
    assert_eq!(malformed_type.font_size, Some(18.0));
    assert_eq!(malformed_type.font_family, Some(9));
    assert_eq!(malformed_type.line_height, Some(1.3));
    assert!((text_primitive(&data, "malformed-type").line_height - 23.4).abs() < 1e-10);
}
