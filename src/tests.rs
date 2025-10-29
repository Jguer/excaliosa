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
            strokeColor: "#000000".to_string(),
            backgroundColor: "none".to_string(),
            fillStyle: "hachure".to_string(),
            strokeWidth: 1.0,
            strokeStyle: "solid".to_string(),
            roughness: 0.0,
            opacity: 100.0,
            groupIds: vec![],
            frameId: None,
            index: "a0".to_string(),
            roundness: None,
            seed: 0,
            versionNonce: Some(0),
            isDeleted: false,
            boundElements: None,
            updated: 0,
            link: None,
            locked: false,
            text: None,
            fontSize: None,
            fontFamily: None,
            textAlign: None,
            verticalAlign: None,
            containerId: None,
            originalText: None,
            lineHeight: None,
            baseline: None,
            startBinding: None,
            endBinding: None,
            startArrowType: None,
            endArrowType: None,
            startArrowhead: None,
            endArrowhead: None,
            points: None,
            lastCommittedPoint: None,
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
        element2.isDeleted = true;

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
            version: 2,
            versionNonce: None,
            source: "test".to_string(),
            elements: vec![element],
            appState: HashMap::new(),
            files: HashMap::new(),
        };

        let svg = generate_svg(&data);

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
            version: 2,
            versionNonce: None,
            source: "test".to_string(),
            elements: vec![],
            appState: HashMap::new(),
            files: HashMap::new(),
        };

        let svg = generate_svg(&data);

        assert!(svg.contains("<svg"));
        assert!(svg.contains("viewBox"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn test_rectangle_rendering() {
        let mut element = create_test_element("rect1", "rectangle", 100.0, 100.0, 200.0, 150.0);
        element.strokeColor = "#ff0000".to_string();
        element.backgroundColor = "#00ff00".to_string();

        let data = ExcalidrawData {
            data_type: "excalidraw".to_string(),
            version: 2,
            versionNonce: None,
            source: "test".to_string(),
            elements: vec![element],
            appState: HashMap::new(),
            files: HashMap::new(),
        };

        let svg = generate_svg(&data);
        assert!(svg.contains("<rect"));
        assert!(svg.contains("width=\"200\""));
        assert!(svg.contains("height=\"150\""));
    }

    #[test]
    fn test_ellipse_rendering() {
        let element = create_test_element("circle1", "ellipse", 200.0, 200.0, 100.0, 100.0);
        let data = ExcalidrawData {
            data_type: "excalidraw".to_string(),
            version: 2,
            versionNonce: None,
            source: "test".to_string(),
            elements: vec![element],
            appState: HashMap::new(),
            files: HashMap::new(),
        };

        let svg = generate_svg(&data);
        assert!(svg.contains("<ellipse"));
        assert!(svg.contains("cx=\"250\""));
        assert!(svg.contains("cy=\"250\""));
    }

    #[test]
    fn test_text_rendering() {
        let mut element = create_test_element("text1", "text", 100.0, 100.0, 100.0, 40.0);
        element.text = Some("Hello World".to_string());
        element.fontSize = Some(16.0);

        let data = ExcalidrawData {
            data_type: "excalidraw".to_string(),
            version: 2,
            versionNonce: None,
            source: "test".to_string(),
            elements: vec![element],
            appState: HashMap::new(),
            files: HashMap::new(),
        };

        let svg = generate_svg(&data);
        assert!(svg.contains("<text"));
        assert!(svg.contains("Hello World"));
    }
}
