#[cfg(test)]
mod renderer_tests {
    use crate::models::{ArrowheadField, ExcalidrawData, ExcalidrawElement};
    use crate::backend::svg::generate_svg;
    use crate::scene::Primitive;
    use crate::utils::calculate_viewbox;
    use std::collections::HashMap;
    use serde_json::json;

    fn create_test_element(
        id: &str,
        element_type: &str,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> ExcalidrawElement {
        ExcalidrawElement {
            id: id.to_string(),
            element_type: element_type.to_string(),
            x,
            y,
            width,
            height,
            stroke_color: "#000000".to_string(),
            background_color: "none".to_string(),
            fill_style: "solid".to_string(),
            stroke_width: 1.0,
            stroke_style: "solid".to_string(),
            roughness: 0.0,
            opacity: 100.0,
            index: "a0".to_string(),
            version_nonce: Some(0),
            ..Default::default()
        }
    }

    fn wrap(elements: Vec<ExcalidrawElement>) -> ExcalidrawData {
        ExcalidrawData {
            data_type: "excalidraw".to_string(),
            version: Some(2),
            version_nonce: None,
            source: Some("test".to_string()),
            elements,
            app_state: HashMap::new(),
            files: HashMap::new(),
        }
    }

    #[test]
    fn test_calculate_viewbox_empty() {
        let elements = vec![];
        let viewbox = calculate_viewbox(&elements);

        assert_eq!(viewbox.min_x, 0.0);
        assert_eq!(viewbox.min_y, 0.0);
        assert_eq!(viewbox.width, 800.0);
        assert_eq!(viewbox.height, 600.0);
    }

    #[test]
    fn test_calculate_viewbox_single_element() {
        let elements = vec![create_test_element("rect1", "rectangle", 100.0, 100.0, 200.0, 150.0)];
        let viewbox = calculate_viewbox(&elements);

        assert_eq!(viewbox.min_x, 60.0); // 100 - 40 padding
        assert_eq!(viewbox.min_y, 60.0);
        assert_eq!(viewbox.width, 280.0); // 200 + 80 padding
        assert_eq!(viewbox.height, 230.0); // 150 + 80 padding
    }

    #[test]
    fn test_calculate_viewbox_ignores_deleted() {
        let element1 = create_test_element("rect1", "rectangle", 100.0, 100.0, 200.0, 150.0);
        let mut element2 = create_test_element("rect2", "rectangle", 500.0, 500.0, 100.0, 100.0);
        element2.is_deleted = true;

        let elements = vec![element1, element2];
        let viewbox = calculate_viewbox(&elements);

        // Should only consider element1 since element2 is deleted
        assert_eq!(viewbox.min_x, 60.0);
        assert_eq!(viewbox.min_y, 60.0);
        assert_eq!(viewbox.width, 280.0);
        assert_eq!(viewbox.height, 230.0);
    }

    #[test]
    fn test_generate_svg_basic() {
        let element = create_test_element("rect1", "rectangle", 100.0, 100.0, 200.0, 150.0);
        let data = ExcalidrawData {
            data_type: "excalidraw".to_string(),
            version: Some(2),
            version_nonce: None,
            source: Some("test".to_string()),
            elements: vec![element],
            app_state: HashMap::new(),
            files: HashMap::new(),
        };

    let svg = generate_svg(&data, None);

        assert!(svg.contains("<svg"));
        assert!(svg.contains("viewBox"));
        assert!(svg.contains("</svg>"));
        // Shapes are emitted as rough <path> geometry (shared with the raster backend).
        assert!(svg.contains("<path"));
    }

    #[test]
    fn test_generate_svg_empty() {
        let data = ExcalidrawData {
            data_type: "excalidraw".to_string(),
            version: Some(2),
            version_nonce: None,
            source: Some("test".to_string()),
            elements: vec![],
            app_state: HashMap::new(),
            files: HashMap::new(),
        };

    let svg = generate_svg(&data, None);

        assert!(svg.contains("<svg"));
        assert!(svg.contains("viewBox"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn test_rectangle_rendering() {
        let mut element = create_test_element("rect1", "rectangle", 100.0, 100.0, 200.0, 150.0);
        element.stroke_color = "#ff0000".to_string();
        element.background_color = "#00ff00".to_string();

        let data = ExcalidrawData {
            data_type: "excalidraw".to_string(),
            version: Some(2),
            version_nonce: None,
            source: Some("test".to_string()),
            elements: vec![element],
            app_state: HashMap::new(),
            files: HashMap::new(),
        };

    let svg = generate_svg(&data, None);
        // Rectangle is now rough path geometry with both fill and stroke colors.
        assert!(svg.contains("<path"));
        assert!(svg.contains("stroke=\"#ff0000\""));
        assert!(svg.contains("fill=\"#00ff00\""));
    }

    #[test]
    fn test_ellipse_rendering() {
        let element = create_test_element("circle1", "ellipse", 200.0, 200.0, 100.0, 100.0);
        let data = ExcalidrawData {
            data_type: "excalidraw".to_string(),
            version: Some(2),
            version_nonce: None,
            source: Some("test".to_string()),
            elements: vec![element],
            app_state: HashMap::new(),
            files: HashMap::new(),
        };

    let svg = generate_svg(&data, None);
        // Ellipse is now rough path geometry rather than an <ellipse> primitive.
        assert!(svg.contains("<path"));
    }

    #[test]
    fn test_text_rendering() {
        let mut element = create_test_element("text1", "text", 100.0, 100.0, 100.0, 40.0);
        element.text = Some("Hello World".to_string());
        element.font_size = Some(16.0);

        let data = ExcalidrawData {
            data_type: "excalidraw".to_string(),
            version: Some(2),
            version_nonce: None,
            source: Some("test".to_string()),
            elements: vec![element],
            app_state: HashMap::new(),
            files: HashMap::new(),
        };

    let svg = generate_svg(&data, None);
        assert!(svg.contains("<text"));
        assert!(svg.contains("Hello World"));
    }

    #[test]
    fn test_transparent_background() {
        // Test rectangle with transparent background - should have no fill
        let mut element = create_test_element("rect1", "rectangle", 100.0, 100.0, 200.0, 150.0);
        element.stroke_color = "#000000".to_string();
        element.background_color = "transparent".to_string();
        element.stroke_width = 2.0;

        let data = ExcalidrawData {
            data_type: "excalidraw".to_string(),
            version: Some(2),
            version_nonce: None,
            source: Some("test".to_string()),
            elements: vec![element],
            app_state: HashMap::new(),
            files: HashMap::new(),
        };

    let svg = generate_svg(&data, None);
        assert!(svg.contains("fill=\"none\""), "Rectangle with transparent background should have fill=\"none\"");
        assert!(svg.contains("stroke=\"#000000\""), "Rectangle should have stroke color");
    }

    #[test]
    fn test_transparent_stroke() {
        // Test rectangle with transparent stroke - should have no stroke
        let mut element = create_test_element("rect1", "rectangle", 100.0, 100.0, 200.0, 150.0);
        element.stroke_color = "transparent".to_string();
        element.background_color = "#ff0000".to_string();
        element.stroke_width = 0.0;

        let data = ExcalidrawData {
            data_type: "excalidraw".to_string(),
            version: Some(2),
            version_nonce: None,
            source: Some("test".to_string()),
            elements: vec![element],
            app_state: HashMap::new(),
            files: HashMap::new(),
        };

    let svg = generate_svg(&data, None);
        assert!(svg.contains("fill=\"#ff0000\""), "Rectangle should have fill color");
        assert!(svg.contains("stroke=\"none\""), "Rectangle with transparent stroke should have stroke=\"none\"");
    }

    #[test]
    fn test_both_stroke_and_fill() {
        // Test rectangle with both stroke and fill
        let mut element = create_test_element("rect1", "rectangle", 100.0, 100.0, 200.0, 150.0);
        element.stroke_color = "#000000".to_string();
        element.background_color = "#dbeafe".to_string();
        element.stroke_width = 2.0;

        let data = ExcalidrawData {
            data_type: "excalidraw".to_string(),
            version: Some(2),
            version_nonce: None,
            source: Some("test".to_string()),
            elements: vec![element],
            app_state: HashMap::new(),
            files: HashMap::new(),
        };

    let svg = generate_svg(&data, None);
        assert!(svg.contains("fill=\"#dbeafe\""), "Rectangle should have fill color");
        assert!(svg.contains("stroke=\"#000000\""), "Rectangle should have stroke color");
    }

    #[test]
    fn test_hachure_fill_style() {
        // Test rectangle with hachure fill style
        let mut element = create_test_element("rect1", "rectangle", 0.0, 0.0, 100.0, 100.0);
        element.fill_style = "hachure".to_string();
        element.background_color = "#868e96".to_string();
        element.stroke_color = "#1e1e1e".to_string();
        element.stroke_width = 2.0;

        let data = ExcalidrawData {
            data_type: "excalidraw".to_string(),
            version: Some(2),
            version_nonce: None,
            source: Some("test".to_string()),
            elements: vec![element],
            app_state: HashMap::new(),
            files: HashMap::new(),
        };

    let svg = generate_svg(&data, None);
        // Should have a pattern path with diagonal lines (multiple M and L commands)
        assert!(svg.contains("stroke=\"#868e96\""), "Hachure pattern should use backgroundColor as stroke");
        assert!(svg.contains("fill=\"none\""), "Hachure pattern path should have fill=\"none\"");
        // Should have border path with stroke color
        assert!(svg.contains("stroke=\"#1e1e1e\""), "Border should use strokeColor");
        // Pattern should have multiple line segments
        assert!(svg.matches("M").count() > 10, "Hachure pattern should have multiple line segments");
    }

    #[test]
    fn test_dotted_line_caps_use_a_tighter_dash() {
        let mut element = create_test_element("arrow1", "arrow", 0.0, 0.0, 0.0, 0.0);
        element.stroke_color = "#000000".to_string();
        element.stroke_style = "dotted".to_string();
        element.stroke_width = 2.0;
        element.points = Some(vec![(0.0, 0.0), (100.0, 0.0)]);
        element.start_arrowhead = ArrowheadField::Named("arrow".to_string());
        element.end_arrowhead = ArrowheadField::Named("arrow".to_string());

        let data = ExcalidrawData {
            data_type: "excalidraw".to_string(),
            version: Some(2),
            version_nonce: None,
            source: Some("test".to_string()),
            elements: vec![element],
            app_state: HashMap::new(),
            files: HashMap::new(),
        };

        let svg = generate_svg(&data, None);

        assert_eq!(svg.matches("stroke-dasharray=\"1.5,8\"").count(), 1);
        assert_eq!(svg.matches("stroke-dasharray=\"1.5,6\"").count(), 4);
    }

    #[test]
    fn test_image_element_renders_as_svg_image() {
        let mut element = create_test_element("img1", "image", 10.0, 20.0, 100.0, 50.0);
        element.file_id = Some("file1".to_string());

        let mut files = HashMap::new();
        files.insert(
            "file1".to_string(),
            json!({
                "mimeType": "image/png",
                "id": "file1",
                // minimal valid PNG (1x1 transparent) data URL
                "dataURL": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMB/6X7vQAAAABJRU5ErkJggg=="
            }),
        );

        let data = ExcalidrawData {
            data_type: "excalidraw".to_string(),
            version: Some(2),
            version_nonce: None,
            source: Some("test".to_string()),
            elements: vec![element],
            app_state: HashMap::new(),
            files,
        };

        let svg = generate_svg(&data, None);
        assert!(svg.contains("<image"), "SVG should contain an <image> element");
        assert!(svg.contains("href=\"data:image/png;base64,"), "SVG image should embed the data URL");
        assert!(svg.contains("width=\"1\""));
        assert!(svg.contains("height=\"1\""));
        assert!(svg.contains(
            "<use href=\"#excaliosa-image-asset-0\" transform=\"translate(10 20) scale(100 50)\"/>"
        ));
    }

    /// Regression: PNG output must be straight (non-premultiplied) alpha. A
    /// semi-transparent fill should keep its full color channels, not be
    /// darkened toward black as premultiplied bytes would be.
    #[test]
    fn test_png_alpha_is_not_premultiplied() {
        let mut el = create_test_element("r", "rectangle", 100.0, 100.0, 200.0, 150.0);
        el.background_color = "#ff0000".to_string();
        el.stroke_color = "transparent".to_string();
        el.stroke_width = 0.0;
        el.opacity = 50.0; // half transparent -> alpha ~127
        let data = wrap(vec![el]);

        let path = std::env::temp_dir().join("excaliosa_alpha_regression.png");
        crate::render_to_png(&data, &path, Some((0, 0, 0, 0)), 75, None).unwrap();

        let decoder = png::Decoder::new(std::fs::File::open(&path).unwrap());
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);

        // A red pixel at ~50% opacity must be stored as (255, 0, 0, ~127).
        // The premultiplied bug would instead store red ~127.
        let found = buf.chunks_exact(4).any(|px| {
            px[0] > 200 && px[1] < 40 && px[2] < 40 && (90..=160).contains(&px[3])
        });
        assert!(found, "expected a straight-alpha semi-transparent red pixel");

        let _ = std::fs::remove_file(&path);
    }

    /// Regression: arrows that use the legacy `end_arrow_type` field (rather
    /// than `end_arrowhead`) must still render an arrowhead in the shared scene.
    #[test]
    fn test_arrow_type_fallback_renders_head() {
        let mut with_head = create_test_element("a", "arrow", 0.0, 0.0, 100.0, 0.0);
        with_head.points = Some(vec![(0.0, 0.0), (100.0, 0.0)]);
        with_head.end_arrow_type = Some("arrow".to_string());

        let mut plain = create_test_element("b", "arrow", 0.0, 0.0, 100.0, 0.0);
        plain.points = Some(vec![(0.0, 0.0), (100.0, 0.0)]);
        plain.end_arrowhead = ArrowheadField::Null;

        let count = |el: ExcalidrawElement| {
            let scene = crate::build_scene(&wrap(vec![el]), None);
            match &scene.drawables[0].primitive {
                Primitive::Paths(paths) => paths.len(),
                _ => 0,
            }
        };

        assert!(
            count(with_head) > count(plain),
            "end_arrow_type must add an arrowhead path"
        );
    }

    /// The SVG and Skia backends consume the same scene, so every non-deleted,
    /// supported element becomes exactly one drawable that both backends render.
    #[test]
    fn test_backends_share_scene() {
        let rect = create_test_element("r", "rectangle", 0.0, 0.0, 50.0, 50.0);
        let ell = create_test_element("e", "ellipse", 100.0, 0.0, 50.0, 50.0);
        let mut deleted = create_test_element("d", "rectangle", 0.0, 200.0, 50.0, 50.0);
        deleted.is_deleted = true;
        let data = wrap(vec![rect, ell, deleted]);

        let scene = crate::build_scene(&data, None);
        assert_eq!(scene.drawables.len(), 2, "deleted elements are dropped once");

        // Both backends emit a <path> per shape, from the same geometry.
        let svg = generate_svg(&data, None);
        assert!(svg.matches("<path").count() >= 2);
    }
}

