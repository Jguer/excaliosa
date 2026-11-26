use excaliosa::color_utils::{parse_color, parse_color_result};
use excaliosa::font_utils::get_font_family;
use excaliosa::{ExcalidrawData, generate_svg};
use serde_json::{Value, json};

fn svg_for(element: Value) -> String {
    let doc: ExcalidrawData =
        serde_json::from_value(json!({"type": "excalidraw", "elements": [element]})).unwrap();
    generate_svg(&doc, None)
}

#[test]
fn numeric_font_ids_select_bundled_fonts() {
    for (id, family) in [
        (None, "Excalifont"),
        (Some(1), "Excalifont"), // Virgil is not bundled
        (Some(2), "Liberation Sans"),
        (Some(3), "Cascadia Code"),
        (Some(5), "Excalifont"),
        (Some(9), "Liberation Sans"),
        (Some(123), "Excalifont"),
    ] {
        assert_eq!(get_font_family(id), family, "{id:?}");
    }
}

#[test]
fn svg_text_uses_top_anchored_baselines_and_keeps_both_opacities() {
    for align in ["top", "middle", "bottom"] {
        let svg = svg_for(json!({
            "type": "text",
            "text": "first\nsecond",
            "y": 100,
            "width": 90,
            "height": 60,
            "fontFamily": 5,
            "fontSize": 20,
            "lineHeight": 1.5,
            "verticalAlign": align,
            "strokeColor": "#1a2b",
            "opacity": 50,
        }));
        assert!(svg.contains("font-family=\"Excalifont\""), "{svg}");
        assert!(svg.contains("y=\"120.12\""), "{svg}");
        assert!(svg.contains("y=\"150.12\""), "{svg}");
        assert!(
            svg.contains("fill=\"#11aa22\" fill-opacity=\"0.7333\""),
            "{svg}"
        );
        assert!(svg.contains("<g opacity=\"0.5000\">"), "{svg}");
    }
}

#[test]
fn shorthand_is_supported_for_elements_not_cli_background() {
    assert_eq!(parse_color("#Ab3"), (170, 187, 51, 255));
    assert_eq!(parse_color("#Ab30"), (170, 187, 51, 0));
    assert_eq!(
        parse_color_result("#aabbcc80").unwrap(),
        (170, 187, 204, 128)
    );
    assert_eq!(parse_color_result("#aabbcc").unwrap(), (170, 187, 204, 255));
    assert_eq!(parse_color_result("transparent").unwrap(), (0, 0, 0, 0));
    for invalid in ["#abc", "#abcd", "none", "#é1234", "#+12345", "#aabbcg"] {
        assert!(parse_color_result(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn shapes_without_valid_visible_background_do_not_get_black_fills() {
    for background in [
        "",
        "none",
        "transparent",
        "#zzzzzz",
        "1é123",
        "#1230",
        "#12345600",
    ] {
        let svg = svg_for(json!({
            "type": "rectangle",
            "width": 50,
            "height": 40,
            "backgroundColor": background,
            "fillStyle": "solid",
        }));
        assert!(svg.contains("fill=\"none\" stroke=\"#1e1e1e\""), "{svg}");
        assert!(!svg.contains("fill=\"#"), "{svg}");
    }
    let svg = svg_for(json!({
        "type": "rectangle",
        "width": 50,
        "height": 40,
        "backgroundColor": "#aB38",
        "fillStyle": "solid",
    }));
    assert!(svg.contains("fill=\"#aabb33\""), "{svg}");
    assert!(svg.contains("fill-opacity=\"0.5333\""), "{svg}");
}
