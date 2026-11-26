use excaliosa::{ExcalidrawData, generate_svg, render_to_png};
use serde_json::{Value, json};

fn fixture() -> ExcalidrawData {
    serde_json::from_str(include_str!("fixtures/svg_links.json")).unwrap()
}

fn rectangle_with_link(link: Option<&str>) -> ExcalidrawData {
    serde_json::from_value(json!({
        "type": "excalidraw",
        "elements": [{
            "type": "rectangle",
            "x": 10,
            "y": 20,
            "width": 100,
            "height": 50,
            "strokeColor": "#123456",
            "backgroundColor": "#abcdef",
            "fillStyle": "solid",
            "roughness": 0,
            "link": link,
        }]
    }))
    .unwrap()
}

fn svg_for_element(element: Value) -> String {
    let data: ExcalidrawData =
        serde_json::from_value(json!({"type": "excalidraw", "elements": [element]})).unwrap();
    generate_svg(&data, None)
}

fn anchor_body<'a>(svg: &'a str, href: &str) -> &'a str {
    let start = format!("<a href=\"{href}\">\n");
    svg.split_once(&start)
        .and_then(|(_, rest)| rest.split_once("\n</a>"))
        .map(|(body, _)| body)
        .unwrap_or_else(|| panic!("missing anchor {href:?} in {svg}"))
}

#[test]
fn safe_links_wrap_their_visible_marks_without_changing_paint_state() {
    let svg = generate_svg(&fixture(), None);
    assert_eq!(svg.matches("<a href=").count(), 3, "{svg}");
    assert!(!svg.contains("target="), "{svg}");
    assert!(!svg.contains("javascript:"), "{svg}");

    let https = anchor_body(&svg, "HtTpS://Example.COM/search?a=1&amp;b=2");
    assert!(
        https.starts_with("<g transform=\"rotate(10 90 55)\">"),
        "{https}"
    );
    assert!(https.ends_with("</g>"), "{https}");
    assert!(
        https.contains("stroke=\"#112233\" stroke-width=\"2\" stroke-opacity=\"0.6500\""),
        "{https}"
    );
    assert!(https.contains("fill-opacity=\"0.6500\""), "{https}");
    assert!(!https.contains("stroke=\"#2244dd\""), "{https}");

    let mailto = anchor_body(&svg, "MAILTO:user+export@example.com");
    assert!(mailto.starts_with("<text "), "{mailto}");
    assert!(mailto.contains("Email exports"), "{mailto}");
    let fragment = anchor_body(&svg, "#details");
    assert!(fragment.contains("stroke=\"#228833\""), "{fragment}");

    let https_pos = svg.find("HtTpS://").unwrap();
    let mailto_pos = svg.find("MAILTO:").unwrap();
    let fragment_pos = svg.find("#details").unwrap();
    let rejected_mark_pos = svg.find("stroke=\"#aa2244\"").unwrap();
    assert!(https_pos < mailto_pos && mailto_pos < fragment_pos);
    assert!(fragment_pos < rejected_mark_pos);
}

#[test]
fn rejected_links_leave_exactly_the_same_svg_artwork() {
    let expected = generate_svg(&rectangle_with_link(None), None);
    for link in [
        "",
        "   ",
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "java\tscript:alert(1)",
        "jav&#x61;script:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "file:///etc/passwd",
        "//example.com/path",
        "/relative/path",
        "https://",
        "https:///path",
        "https://user:pass@example.com/",
        "https://example.com@evil.test/",
        "https://example.com\\@evil.test/",
        "https://exa mple.com/",
        "https://example.com/\nonclick=alert(1)",
        "https://example.com/\u{0}",
        "https://example.com/\u{fffe}",
        "https://example.com/#\u{ffff}",
        "https://example.com/\" onclick=\"alert(1)",
        "https://example.com/' onclick='alert(1)",
        "https://example.com:invalid/",
        "https://:443/",
        "https://exa_mple.com/",
        "https://192.0.513/",
        "mailto:not-an-address",
        "mailto:user%@example.com",
        "mailto:user@@example.com",
        "mailto:user@example.com?subject=hello",
        "#",
        "#one#two",
    ] {
        let actual = generate_svg(&rectangle_with_link(Some(link)), None);
        assert_eq!(actual, expected, "link should be rejected: {link:?}");
    }
}

#[test]
fn standard_ipv4_hosts_remain_valid() {
    let href = "https://192.0.2.1/";
    let svg = generate_svg(&rectangle_with_link(Some(href)), None);
    anchor_body(&svg, href);
}

#[test]
fn zero_opacity_marks_keep_their_artwork_without_anchors() {
    let mut linked = rectangle_with_link(Some("https://example.com/invisible"));
    let mut unlinked = rectangle_with_link(None);
    linked.elements[0].opacity = 0.0;
    unlinked.elements[0].opacity = 0.0;

    let actual = generate_svg(&linked, None);
    assert_eq!(actual, generate_svg(&unlinked, None));
    assert!(actual.contains("fill-opacity=\"0.0000\""), "{actual}");
    assert!(actual.contains("stroke-opacity=\"0.0000\""), "{actual}");
    assert!(!actual.contains("<g opacity="), "{actual}");
}

#[test]
fn skipped_or_deleted_marks_do_not_emit_empty_anchors() {
    for element in [
        json!({
            "type": "rectangle",
            "width": 20,
            "height": 20,
            "isDeleted": true,
            "link": "https://example.com/deleted"
        }),
        json!({
            "type": "text",
            "width": 20,
            "height": 20,
            "text": "",
            "link": "#empty"
        }),
        json!({
            "type": "image",
            "width": 20,
            "height": 20,
            "fileId": "missing",
            "link": "https://example.com/missing"
        }),
        json!({
            "type": "freedraw",
            "width": 20,
            "height": 20,
            "link": "#unsupported"
        }),
    ] {
        let svg = svg_for_element(element);
        assert!(!svg.contains("<a href="), "{svg}");
    }
}

#[test]
fn link_metadata_does_not_change_decoded_png_pixels() {
    let linked = fixture();
    let mut unlinked = fixture();
    for element in &mut unlinked.elements {
        element.link = None;
    }

    let dir = tempfile::tempdir().unwrap();
    let linked_path = dir.path().join("linked.png");
    let unlinked_path = dir.path().join("unlinked.png");
    render_to_png(&linked, &linked_path, None, 75, None).unwrap();
    render_to_png(&unlinked, &unlinked_path, None, 75, None).unwrap();

    let linked = tiny_skia::Pixmap::load_png(linked_path).unwrap();
    let unlinked = tiny_skia::Pixmap::load_png(unlinked_path).unwrap();
    assert_eq!(
        (linked.width(), linked.height()),
        (unlinked.width(), unlinked.height())
    );
    assert_eq!(linked.data(), unlinked.data());
}
