#[cfg(test)]
mod renderer_tests {
    use crate::models::{ExcalidrawData, ExcalidrawElement};
    use crate::renderer::{calculate_viewbox, generate_svg};
    use std::collections::HashMap;

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
            angle: 0.0,
            stroke_color: "#000000".to_string(),
            background_color: "none".to_string(),
            fill_style: "solid".to_string(),
            stroke_width: 1.0,
            stroke_style: "solid".to_string(),
            roughness: 0.0,
            opacity: 100.0,
            group_ids: vec![],
            frame_id: None,
            index: "a0".to_string(),
            roundness: None,
            seed: 0,
            version_nonce: Some(0),
            is_deleted: false,
            bound_elements: None,
            updated: 0,
            link: None,
            locked: false,
            text: None,
            font_size: None,
            font_family: None,
            text_align: None,
            vertical_align: None,
            container_id: None,
            original_text: None,
            line_height: None,
            baseline: None,
            start_binding: None,
            end_binding: None,
            start_arrow_type: None,
            end_arrow_type: None,
            start_arrowhead: None,
            end_arrowhead: None,
            points: None,
            last_committed_point: None,
            elbowed: None,
            version: None,
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
        assert!(svg.contains("<rect"));
        assert!(svg.contains("<marker"));
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
        assert!(svg.contains("<rect"));
        assert!(svg.contains("width=\"200\""));
        assert!(svg.contains("height=\"150\""));
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
        assert!(svg.contains("<ellipse"));
        assert!(svg.contains("cx=\"250\""));
        assert!(svg.contains("cy=\"250\""));
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
    fn test_arrowheads_are_solid_even_when_dotted() {
        // Create an arrow with dotted stroke and arrowheads
        let mut element = create_test_element("arrow1", "arrow", 0.0, 0.0, 0.0, 0.0);
        element.stroke_color = "#000000".to_string();
        element.stroke_style = "dotted".to_string();
        element.stroke_width = 2.0;
        element.points = Some(vec![(0.0, 0.0), (100.0, 0.0)]);
        element.start_arrowhead = Some("arrow".to_string());
        element.end_arrowhead = Some("arrow".to_string());

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

        // There should be exactly one stroke-dasharray attribute (on the shaft),
        // but NOT on arrowheads which should remain solid.
        let dasharray_count = svg.matches("stroke-dasharray=\"").count();
        assert_eq!(dasharray_count, 1, "Only the shaft should be dashed, arrowheads must be solid");
    }
}

