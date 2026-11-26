use base64::Engine;
use excaliosa::font_utils::normalize_newlines;
use excaliosa::fonts::bundled_font;
use excaliosa::{ExcalidrawData, generate_svg, render_to_png};
use serde_json::{Value, json};

fn document(elements: Value) -> ExcalidrawData {
    serde_json::from_value(json!({"type": "excalidraw", "elements": elements})).unwrap()
}

fn text_element(text: &str, align: &str) -> Value {
    json!({
        "type": "text",
        "text": text,
        "x": 10,
        "y": 20,
        "width": 100,
        "height": 150,
        "fontFamily": 5,
        "fontSize": 24,
        "lineHeight": 1.5,
        "textAlign": align
    })
}

fn text_nodes(svg: &str) -> Vec<&str> {
    svg.split("<text ")
        .skip(1)
        .map(|rest| rest.split("</text>").next().unwrap())
        .collect()
}

#[test]
fn both_exports_normalize_newlines_and_svg_preserves_whitespace() {
    let saved = "  A & <tag> \"quoted\" 'apostrophe'  \r\n\rthird\r\n\n  last  ";
    let normalized = normalize_newlines(saved);
    let data = document(json!([text_element(saved, "left")]));
    let svg = generate_svg(&data, None);

    assert!(!svg.contains('\r'));
    let nodes = text_nodes(&svg);
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].matches("<tspan ").count(), 5);
    assert_eq!(nodes[0].matches("></tspan>").count(), 2);
    assert!(
        nodes[0]
            .contains(">  A &amp; &lt;tag&gt; &quot;quoted&quot; &apos;apostrophe&apos;  </tspan>")
    );
    assert!(nodes[0].contains(">third</tspan>"));
    assert!(nodes[0].contains(">  last  </tspan>"));
    for y in ["44.14", "80.14", "116.14", "152.14", "188.14"] {
        assert!(
            nodes[0].contains(&format!("y=\"{y}\"")),
            "missing baseline {y}"
        );
    }

    let dir = tempfile::tempdir().unwrap();
    let saved_png = dir.path().join("saved.png");
    let normalized_png = dir.path().join("normalized.png");
    render_to_png(&data, &saved_png, None, 75, None).unwrap();
    let normalized_data = document(json!([text_element(&normalized, "left")]));
    render_to_png(&normalized_data, &normalized_png, None, 75, None).unwrap();
    assert_eq!(
        std::fs::read(saved_png).unwrap(),
        std::fs::read(normalized_png).unwrap()
    );
}

#[test]
fn svg_direction_uses_first_directional_character_and_logical_anchors() {
    let data = document(json!([
        text_element("123 שלום", "left"),
        text_element("123 שלום", "center"),
        text_element("123 שלום", "right"),
        text_element("123 English", "left"),
        text_element("123 English", "right"),
    ]));
    let svg = generate_svg(&data, None);
    let nodes = text_nodes(&svg);

    for (node, anchor, x) in [
        (nodes[0], "end", "10.00"),
        (nodes[1], "middle", "60.00"),
        (nodes[2], "start", "110.00"),
    ] {
        assert!(node.contains("direction=\"rtl\""));
        assert!(node.contains(&format!("text-anchor=\"{anchor}\"")));
        assert!(node.contains(&format!("x=\"{x}\"")));
    }
    assert!(nodes[3].contains("direction=\"ltr\""));
    assert!(nodes[3].contains("text-anchor=\"start\""));
    assert!(nodes[3].contains("x=\"10.00\""));
    assert!(nodes[4].contains("direction=\"ltr\""));
    assert!(nodes[4].contains("text-anchor=\"end\""));
    assert!(nodes[4].contains("x=\"110.00\""));
}

#[test]
fn svg_embeds_each_approved_used_face_once_with_generic_fallbacks() {
    let data: ExcalidrawData =
        serde_json::from_str(include_str!("fixtures/portable_text.json")).unwrap();
    let svg = generate_svg(&data, None);
    assert_eq!(svg, generate_svg(&data, None));
    assert_eq!(svg.matches("@font-face").count(), 2);
    assert!(svg.contains("font-family=\"Excalifont, sans-serif\""));
    assert!(svg.contains("font-family=\"Cascadia Code, monospace\""));
    assert!(svg.contains("font-family=\"Liberation Sans, sans-serif\""));

    let css = svg
        .split("<style class=\"style-fonts\">\n")
        .nth(1)
        .unwrap()
        .split("    </style>")
        .next()
        .unwrap();
    assert!(!css.contains("Liberation Sans"));
    for family in ["Excalifont", "Cascadia Code"] {
        let prefix =
            format!("@font-face {{ font-family: \"{family}\"; src: url(\"data:font/ttf;base64,");
        assert_eq!(css.matches(&prefix).count(), 1, "{family}");
        let payload = css
            .split(&prefix)
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .unwrap();
        assert_eq!(decoded, bundled_font(family).bytes, "{family}");
    }

    let shape = document(json!([{"type": "rectangle", "width": 20, "height": 20}]));
    assert!(!generate_svg(&shape, None).contains("style-fonts"));
    let fallback_only = document(json!([{
        "type": "text",
        "text": "fallback",
        "fontFamily": 9,
        "width": 100,
        "height": 30
    }]));
    assert!(!generate_svg(&fallback_only, None).contains("style-fonts"));
}
