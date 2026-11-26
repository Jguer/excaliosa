use excaliosa::models::ViewBox;
use excaliosa::scene::PathCompositing;
use excaliosa::{ExcalidrawData, build_scene, generate_svg, render_to_png};

fn fixture() -> ExcalidrawData {
    serde_json::from_str(include_str!("fixtures/translucent_paths.json")).unwrap()
}

#[test]
fn lowering_selects_per_paint_only_for_ordinary_paths() {
    let scene = build_scene(&fixture(), None);
    assert_eq!(scene.drawables.len(), 10);
    for index in [0, 1, 2, 3, 4, 6] {
        assert_eq!(
            scene.drawables[index].path_compositing,
            PathCompositing::PerPaint,
            "drawable {index}"
        );
    }
    for index in [5, 7, 8, 9] {
        assert_eq!(
            scene.drawables[index].path_compositing,
            PathCompositing::Group,
            "drawable {index}"
        );
    }
    assert_eq!(scene.drawables[6].opacity, 0.3);
    assert_eq!(scene.drawables[6].frame_opacity, Some(0.5));
    assert!(scene.drawables[6].frame_clip.is_some());
    assert_eq!(scene.drawables[9].opacity, 0.3);
}

#[test]
fn svg_applies_effective_opacity_to_ordinary_paints_not_their_wrappers() {
    let svg = generate_svg(&fixture(), Some((0, 0, 0, 0)));
    for color in ["#d04050", "#40d050", "#5040d0", "#d05040"] {
        assert!(
            svg.contains(&format!(
                "fill=\"{color}\" stroke=\"none\" fill-opacity=\"0.5000\""
            )),
            "missing translucent fill {color} in {svg}"
        );
    }
    for color in ["#102030", "#103040", "#104050", "#203040", "#304050"] {
        assert!(
            svg.contains(&format!(
                "stroke=\"{color}\" stroke-width=\"20\" stroke-opacity=\"0.5000\""
            )),
            "missing translucent stroke {color} in {svg}"
        );
    }
    assert!(svg.contains("fill=\"#405060\" stroke=\"none\" fill-opacity=\"0.3000\""));
    assert!(svg.contains("stroke=\"#607080\" stroke-width=\"20\" stroke-opacity=\"0.3000\""));

    // Only the two sticky-paper groups flatten their paints before fading.
    assert_eq!(svg.matches("<g opacity=\"0.5000\">").count(), 1, "{svg}");
    assert_eq!(svg.matches("<g opacity=\"0.3000\">").count(), 1, "{svg}");
    assert_eq!(svg.matches("fill=\"#ffd166\" stroke=\"none\"/>").count(), 2);
    assert_eq!(svg.matches("excaliosa-frame-clip-").count(), 4);
}

struct Raster {
    width: usize,
    pixels: Vec<u8>,
}

impl Raster {
    fn alpha(&self, view: ViewBox, x: f64, y: f64) -> u8 {
        let px = (x - view.min_x).floor() as usize;
        let py = (y - view.min_y).floor() as usize;
        self.pixels[(py * self.width + px) * 4 + 3]
    }
}

fn render_fixture() -> (Raster, ViewBox) {
    std::fs::create_dir_all(".pi-cache").unwrap();
    let directory = tempfile::tempdir_in(".pi-cache").unwrap();
    let output = directory.path().join("translucent-paths.png");
    let data = fixture();
    let view = build_scene(&data, None).viewbox;
    render_to_png(&data, &output, Some((0, 0, 0, 0)), 75, None).unwrap();

    let decoder = png::Decoder::new(std::fs::File::open(output).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    pixels.truncate(info.buffer_size());
    (
        Raster {
            width: info.width as usize,
            pixels,
        },
        view,
    )
}

fn assert_overlap_alpha(actual: u8) {
    assert!((191..=192).contains(&actual), "overlap alpha was {actual}");
}

#[test]
fn png_composites_each_ordinary_paint_and_fades_sticky_paper_once() {
    let (raster, view) = render_fixture();
    let alpha = |x, y| raster.alpha(view, x, y);

    for (x, y) in [(80.0, 80.0), (260.0, 80.0), (440.0, 80.0), (260.0, 260.0)] {
        assert_eq!(alpha(x, y), 128, "fill-only probe at {x},{y}");
    }
    for (x, y) in [
        (80.0, 15.0),
        (260.0, 15.0),
        (440.0, 15.0),
        (260.0, 195.0),
        (80.0, 220.0),
    ] {
        assert_eq!(alpha(x, y), 128, "stroke-only probe at {x},{y}");
    }
    for (x, y) in [(80.0, 25.0), (260.0, 25.0), (440.0, 25.0), (260.0, 205.0)] {
        assert_overlap_alpha(alpha(x, y));
    }
    assert!(
        alpha(140.0, 220.0) > 128,
        "arrow cap and shaft must overlap"
    );

    assert!((76..=77).contains(&alpha(480.0, 280.0)));
    assert!((76..=77).contains(&alpha(480.0, 215.0)));
    assert!((130..=131).contains(&alpha(480.0, 225.0)));

    assert!((127..=128).contains(&alpha(700.0, 80.0)));
    assert!((76..=77).contains(&alpha(720.0, 280.0)));
}
