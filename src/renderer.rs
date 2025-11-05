use crate::arrow_utils::calc_arrowhead_points;
use crate::models::{ExcalidrawData, ExcalidrawElement, ViewBox};
use crate::rect_utils::{get_corner_radius, generate_rounded_rect_path};

pub fn calculate_viewbox(elements: &[ExcalidrawElement]) -> ViewBox {
    const PADDING: f64 = 40.0;

    if elements.is_empty() {
        return ViewBox {
            min_x: 0.0,
            min_y: 0.0,
            width: 800.0,
            height: 600.0,
        };
    }

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for el in elements {
        if !el.is_deleted {
            min_x = min_x.min(el.x);
            min_y = min_y.min(el.y);
            max_x = max_x.max(el.x + el.width);
            max_y = max_y.max(el.y + el.height);
        }
    }

    ViewBox {
        min_x: min_x - PADDING,
        min_y: min_y - PADDING,
        width: max_x - min_x + PADDING * 2.0,
        height: max_y - min_y + PADDING * 2.0,
    }
}

// Simple deterministic RNG (LCG) for jitter, seeded by element.seed
struct LcgRng {
    state: u64,
}

impl LcgRng {
    fn new(seed: i32) -> Self {
        // mix the seed a bit and avoid zero
        let mut s = seed as u64;
        s ^= 0x9E3779B97F4A7C15;
        if s == 0 { s = 0xDEADBEEFCAFEBABE; }
        Self { state: s }
    }
    fn next_u64(&mut self) -> u64 {
        // Numerical Recipes LCG parameters
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.state
    }
    fn next_f64(&mut self) -> f64 {
        let v = self.next_u64() >> 11; // 53 bits
        (v as f64) / ((1u64 << 53) as f64)
    }
    fn range(&mut self, min: f64, max: f64) -> f64 {
        min + (max - min) * self.next_f64()
    }
}

// Return smoothed SVG path using Catmull–Rom to cubic Bezier conversion.
// Expects absolute points (x,y). If less than 3 points, fall back to straight segments.
// This matches Excalidraw's "perfect shape" rendering for curved lines.
fn catmull_rom_path(points: &[(f64, f64)]) -> String {
    if points.len() <= 2 {
        // fallback to polyline
        return format!(
            "M {}",
            points
                .iter()
                .map(|(x, y)| format!("{x},{y}"))
                .collect::<Vec<_>>()
                .join(" L ")
        );
    }

    // Helper to get point with endpoint duplication (Catmull-Rom style)
    let get = |i: isize| -> (f64, f64) {
        let n = points.len() as isize;
        let idx = if i < 0 { 0 } else if i >= n { n - 1 } else { i } as usize;
        points[idx]
    };

    let mut d = String::new();
    let (x0, y0) = points[0];
    d.push_str(&format!("M {x0},{y0}"));

    // Catmull-Rom tension parameter (like Excalidraw's default 0.5)
    let tension = 0.5;

    for i in 0..(points.len() - 1) {
        let p0 = get(i as isize - 1);
        let p1 = get(i as isize);
        let p2 = get(i as isize + 1);
        let p3 = get(i as isize + 2);

        // Catmull-Rom to cubic Bezier control points
        // Tangent at p1: (p2 - p0) * tension
        // Tangent at p2: (p3 - p1) * tension
        let tangent1_x = (p2.0 - p0.0) * tension;
        let tangent1_y = (p2.1 - p0.1) * tension;
        let tangent2_x = (p3.0 - p1.0) * tension;
        let tangent2_y = (p3.1 - p1.1) * tension;

        let cp1_x = p1.0 + tangent1_x / 3.0;
        let cp1_y = p1.1 + tangent1_y / 3.0;
        let cp2_x = p2.0 - tangent2_x / 3.0;
        let cp2_y = p2.1 - tangent2_y / 3.0;

        d.push_str(&format!(" C {cp1_x},{cp1_y} {cp2_x},{cp2_y} {},{}", p2.0, p2.1));
    }
    d
}

// Jitter a polyline slightly, offsetting points mostly perpendicular to local direction.
fn jitter_polyline(points: &[(f64, f64)], rng: &mut LcgRng, amplitude: f64) -> Vec<(f64, f64)> {
    if points.len() < 2 { return points.to_vec(); }
    let mut out = Vec::with_capacity(points.len());
    for i in 0..points.len() {
        let (x, y) = points[i];
        // Determine local tangent
        let (nx, ny) = if i == 0 {
            let (x2, y2) = points[1];
            (x2 - x, y2 - y)
        } else if i == points.len() - 1 {
            let (x0, y0) = points[i - 1];
            (x - x0, y - y0)
        } else {
            let (x0, y0) = points[i - 1];
            let (x2, y2) = points[i + 1];
            (x2 - x0, y2 - y0)
        };
        let len = (nx * nx + ny * ny).sqrt().max(1e-6);
        let tx = nx / len;
        let ty = ny / len;
        // Perpendicular
        let px = -ty;
        let py = tx;
        // Offset magnitude with slight tangential component
        let perp = rng.range(-amplitude, amplitude);
        let tang = rng.range(-amplitude * 0.3, amplitude * 0.3);
        out.push((x + px * perp + tx * tang, y + py * perp + ty * tang));
    }
    out
}

/// Generate ellipse points with optional offset (for rough rendering)
/// Based on rough.js _computeEllipsePoints
fn generate_ellipse_points(cx: f64, cy: f64, rx: f64, ry: f64, offset_factor: f64, rng: &mut LcgRng, roughness: f64) -> Vec<(f64, f64)> {
    // Calculate number of points based on perimeter - matching rough.js exactly
    // psq = Math.sqrt(Math.PI * 2 * Math.sqrt((rx^2 + ry^2) / 2))
    let psq = (std::f64::consts::PI * 2.0 * ((rx.powi(2) + ry.powi(2)) / 2.0).sqrt()).sqrt();
    
    // rough.js default curveStepCount is 9
    const CURVE_STEP_COUNT: f64 = 9.0;
    
    // stepCount = Math.ceil(Math.max(curveStepCount, (curveStepCount / Math.sqrt(200)) * psq))
    let step_count = (CURVE_STEP_COUNT.max((CURVE_STEP_COUNT / 200.0_f64.sqrt()) * psq)).ceil() as usize;
    let increment = (std::f64::consts::PI * 2.0) / step_count as f64;
    
    let mut points = Vec::new();
    let rad_offset = rng.range(-0.5, 0.5) - std::f64::consts::PI / 2.0;
    let overlap = increment * 0.5;
    
    // Add starting points for smooth closure
    let start_angle = rad_offset - increment;
    points.push((
        cx + 0.9 * rx * start_angle.cos() + rng.range(-offset_factor, offset_factor) * roughness,
        cy + 0.9 * ry * start_angle.sin() + rng.range(-offset_factor, offset_factor) * roughness,
    ));
    
    // Main ellipse points
    let end_angle = std::f64::consts::PI * 2.0 + rad_offset - 0.01;
    let mut angle = rad_offset;
    while angle < end_angle {
        points.push((
            cx + rx * angle.cos() + rng.range(-offset_factor, offset_factor) * roughness,
            cy + ry * angle.sin() + rng.range(-offset_factor, offset_factor) * roughness,
        ));
        angle += increment;
    }
    
    // Add closing points for smooth overlap
    points.push((
        cx + rx * (rad_offset + std::f64::consts::PI * 2.0 + overlap * 0.5).cos() + rng.range(-offset_factor, offset_factor) * roughness,
        cy + ry * (rad_offset + std::f64::consts::PI * 2.0 + overlap * 0.5).sin() + rng.range(-offset_factor, offset_factor) * roughness,
    ));
    points.push((
        cx + 0.98 * rx * (rad_offset + overlap).cos() + rng.range(-offset_factor, offset_factor) * roughness,
        cy + 0.98 * ry * (rad_offset + overlap).sin() + rng.range(-offset_factor, offset_factor) * roughness,
    ));
    points.push((
        cx + 0.9 * rx * (rad_offset + overlap * 0.5).cos() + rng.range(-offset_factor, offset_factor) * roughness,
        cy + 0.9 * ry * (rad_offset + overlap * 0.5).sin() + rng.range(-offset_factor, offset_factor) * roughness,
    ));
    
    points
}

/// Generate rough polygon paths with multiple passes
fn generate_rough_polygon_paths(points: &[(f64, f64)], roughness: f64, seed: i32) -> Vec<(String, f64)> {
    let mut paths = Vec::new();
    
    if roughness <= 0.0 || points.len() < 3 {
        // No roughness - return simple polygon
        let points_str = points
            .iter()
            .enumerate()
            .map(|(i, (x, y))| {
                if i == 0 {
                    format!("M{x:.2},{y:.2}")
                } else {
                    format!("L{x:.2},{y:.2}")
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        paths.push((format!("{points_str} Z"), 1.0));
        return paths;
    }
    
    let mut rng = LcgRng::new(seed);
    let amplitude = 1.2 * roughness.max(0.0);
    
    // Primary pass - main roughness
    let jittered1 = jitter_polyline(points, &mut rng, amplitude);
    let path1 = jittered1
        .iter()
        .enumerate()
        .map(|(i, (x, y))| {
            if i == 0 {
                format!("M{x:.2},{y:.2}")
            } else {
                format!("L{x:.2},{y:.2}")
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    paths.push((format!("{path1} Z"), 1.0));
    
    // Secondary pass (like rough.js overlay)
    if roughness > 0.0 {
        let mut rng2 = LcgRng::new(seed.wrapping_add(1));
        let jittered2 = jitter_polyline(points, &mut rng2, amplitude * 0.5);
        let path2 = jittered2
            .iter()
            .enumerate()
            .map(|(i, (x, y))| {
                if i == 0 {
                    format!("M{x:.2},{y:.2}")
                } else {
                    format!("L{x:.2},{y:.2}")
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        paths.push((format!("{path2} Z"), 0.85));
    }
    
    // Tertiary pass for high roughness
    if roughness > 1.0 {
        let mut rng3 = LcgRng::new(seed.wrapping_add(2));
        let jittered3 = jitter_polyline(points, &mut rng3, amplitude * 0.3);
        let path3 = jittered3
            .iter()
            .enumerate()
            .map(|(i, (x, y))| {
                if i == 0 {
                    format!("M{x:.2},{y:.2}")
                } else {
                    format!("L{x:.2},{y:.2}")
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        paths.push((format!("{path3} Z"), 0.7));
    }
    
    paths
}

/// Generate rough ellipse paths with multiple passes (based on rough.js)
fn generate_rough_ellipse_paths(cx: f64, cy: f64, rx: f64, ry: f64, roughness: f64, seed: i32) -> Vec<(String, f64)> {
    let mut paths = Vec::new();
    
    if roughness <= 0.0 {
        // No roughness - return perfect ellipse using path
        let path_data = format!(
            "M {} {} A {} {} 0 1 1 {} {} A {} {} 0 1 1 {} {}",
            cx + rx, cy,
            rx, ry,
            cx - rx, cy,
            rx, ry,
            cx + rx, cy
        );
        paths.push((path_data, 1.0));
        return paths;
    }
    
    // Primary pass - main roughness (offset factor 1)
    let mut rng1 = LcgRng::new(seed);
    let points1 = generate_ellipse_points(cx, cy, rx, ry, 1.0, &mut rng1, roughness);
    let path1 = catmull_rom_path(&points1);
    paths.push((path1, 1.0));
    
    // Secondary pass - overlay with more offset (like rough.js with offset 1.5)
    if roughness > 0.0 {
        let mut rng2 = LcgRng::new(seed.wrapping_add(1));
        let points2 = generate_ellipse_points(cx, cy, rx, ry, 1.5, &mut rng2, roughness);
        let path2 = catmull_rom_path(&points2);
        paths.push((path2, 0.85));
    }
    
    // Tertiary pass for high roughness
    if roughness > 1.0 {
        let mut rng3 = LcgRng::new(seed.wrapping_add(2));
        let points3 = generate_ellipse_points(cx, cy, rx, ry, 1.2, &mut rng3, roughness * 0.7);
        let path3 = catmull_rom_path(&points3);
        paths.push((path3, 0.7));
    }
    
    paths
}

fn get_font_family(font_id: Option<i32>) -> &'static str {
    match font_id {
        Some(1) => "Liberation Sans",
        Some(2) => "CascadiaCode",
        _ => "Excalifont",
    }
}

fn get_stroke_dasharray(stroke_style: &str) -> &'static str {
    match stroke_style {
        "dashed" => "8,4",
        "dotted" => "2,2",
        _ => "none",
    }
}

fn get_text_anchor(text_align: Option<&str>) -> &'static str {
    match text_align {
        Some("center") => "middle",
        Some("right") => "end",
        Some("left") => "start",
        _ => "start",
    }
}

fn get_vertical_offset(vertical_align: Option<&str>, font_size: f64) -> f64 {
    match vertical_align {
        Some("middle") => font_size * 0.35,
        Some("bottom") => font_size * 0.9,
        _ => font_size * 0.75,
    }
}

fn get_line_height(font_size: f64, line_height: Option<f64>) -> f64 {
    line_height.unwrap_or(1.25) * font_size
}


/// Generate a single rough line segment using bezier curves (based on rough.js _line)
#[allow(clippy::too_many_arguments)]
fn generate_rough_line_segment(
    x1: f64, y1: f64, x2: f64, y2: f64,
    rng: &mut LcgRng,
    roughness: f64,
    bowing: f64,
    max_offset: f64,
    preserve_vertices: bool,
    is_overlay: bool,
) -> String {
    let length_sq = (x1 - x2).powi(2) + (y1 - y2).powi(2);
    let length = length_sq.sqrt();
    
    // Roughness gain based on length (like rough.js)
    let roughness_gain = if length < 200.0 {
        1.0
    } else if length > 500.0 {
        0.4
    } else {
        (-0.0016668) * length + 1.233334
    };
    
    let mut offset = max_offset;
    if (offset * offset * 100.0) > length_sq {
        offset = length / 10.0;
    }
    
    let half_offset = offset / 2.0;
    let diverge_point = 0.2 + rng.next_f64() * 0.2;
    
    // Bowing creates perpendicular displacement
    let mut mid_disp_x = bowing * max_offset * (y2 - y1) / 200.0;
    let mut mid_disp_y = bowing * max_offset * (x1 - x2) / 200.0;
    mid_disp_x = rng.range(-mid_disp_x, mid_disp_x) * roughness * roughness_gain;
    mid_disp_y = rng.range(-mid_disp_y, mid_disp_y) * roughness * roughness_gain;
    
    // Start point offset
    let (start_x, start_y) = if preserve_vertices {
        (x1, y1)
    } else if is_overlay {
        (
            x1 + rng.range(-half_offset, half_offset) * roughness * roughness_gain,
            y1 + rng.range(-half_offset, half_offset) * roughness * roughness_gain,
        )
    } else {
        (
            x1 + rng.range(-offset, offset) * roughness * roughness_gain,
            y1 + rng.range(-offset, offset) * roughness * roughness_gain,
        )
    };
    
    // End point offset
    let (end_x, end_y) = if preserve_vertices {
        (x2, y2)
    } else if is_overlay {
        (
            x2 + rng.range(-half_offset, half_offset) * roughness * roughness_gain,
            y2 + rng.range(-half_offset, half_offset) * roughness * roughness_gain,
        )
    } else {
        (
            x2 + rng.range(-offset, offset) * roughness * roughness_gain,
            y2 + rng.range(-offset, offset) * roughness * roughness_gain,
        )
    };
    
    // Control points with randomness
    let cp1_x = if is_overlay {
        mid_disp_x + x1 + (x2 - x1) * diverge_point + rng.range(-half_offset, half_offset) * roughness * roughness_gain
    } else {
        mid_disp_x + x1 + (x2 - x1) * diverge_point + rng.range(-offset, offset) * roughness * roughness_gain
    };
    
    let cp1_y = if is_overlay {
        mid_disp_y + y1 + (y2 - y1) * diverge_point + rng.range(-half_offset, half_offset) * roughness * roughness_gain
    } else {
        mid_disp_y + y1 + (y2 - y1) * diverge_point + rng.range(-offset, offset) * roughness * roughness_gain
    };
    
    let cp2_x = if is_overlay {
        mid_disp_x + x1 + 2.0 * (x2 - x1) * diverge_point + rng.range(-half_offset, half_offset) * roughness * roughness_gain
    } else {
        mid_disp_x + x1 + 2.0 * (x2 - x1) * diverge_point + rng.range(-offset, offset) * roughness * roughness_gain
    };
    
    let cp2_y = if is_overlay {
        mid_disp_y + y1 + 2.0 * (y2 - y1) * diverge_point + rng.range(-half_offset, half_offset) * roughness * roughness_gain
    } else {
        mid_disp_y + y1 + 2.0 * (y2 - y1) * diverge_point + rng.range(-offset, offset) * roughness * roughness_gain
    };
    
    format!(
        "M{start_x:.2},{start_y:.2} C{cp1_x:.2},{cp1_y:.2} {cp2_x:.2},{cp2_y:.2} {end_x:.2},{end_y:.2}"
    )
}

/// Generate rough rectangle using linearPath approach (like rough.js)
/// Generate corner points for a rounded rectangle
/// Returns a vec of points that define the rounded rectangle path
fn generate_rounded_rect_points(x: f64, y: f64, width: f64, height: f64, radius: f64) -> Vec<(f64, f64)> {
    let r = radius.min(width / 2.0).min(height / 2.0);
    
    // Generate points along the rounded rectangle perimeter
    // Use more points per corner for smoother rough rendering
    let mut points = Vec::new();
    
    // Increased corner steps for smoother curves (was 5, now 8)
    let corner_steps = 8;
    
    // Top edge: from (x+r, y) to (x+width-r, y)
    points.push((x + r, y));
    
    // Top-right corner arc: from -90° to 0°
    for i in 0..=corner_steps {
        let t = i as f64 / corner_steps as f64;
        let angle = -std::f64::consts::PI / 2.0 + t * std::f64::consts::PI / 2.0;
        points.push((
            x + width - r + r * angle.cos(),
            y + r + r * angle.sin()
        ));
    }
    
    // Right edge: from (x+width, y+r) to (x+width, y+height-r)
    points.push((x + width, y + height - r));
    
    // Bottom-right corner arc: from 0° to 90°
    for i in 0..=corner_steps {
        let t = i as f64 / corner_steps as f64;
        let angle = t * std::f64::consts::PI / 2.0;
        points.push((
            x + width - r + r * angle.cos(),
            y + height - r + r * angle.sin()
        ));
    }
    
    // Bottom edge: from (x+width-r, y+height) to (x+r, y+height)
    points.push((x + r, y + height));
    
    // Bottom-left corner arc: from 90° to 180°
    for i in 0..=corner_steps {
        let t = i as f64 / corner_steps as f64;
        let angle = std::f64::consts::PI / 2.0 + t * std::f64::consts::PI / 2.0;
        points.push((
            x + r + r * angle.cos(),
            y + height - r + r * angle.sin()
        ));
    }
    
    // Left edge: from (x, y+height-r) to (x, y+r)
    points.push((x, y + r));
    
    // Top-left corner arc: from 180° to 270°
    for i in 0..=corner_steps {
        let t = i as f64 / corner_steps as f64;
        let angle = std::f64::consts::PI + t * std::f64::consts::PI / 2.0;
        points.push((
            x + r + r * angle.cos(),
            y + r + r * angle.sin()
        ));
    }
    
    points
}

/// Generate multiple rough rectangle strokes (rough.js style multi-pass)
/// Uses linearPath approach for both rounded and non-rounded rectangles
fn generate_rough_rect_paths(
    x: f64, y: f64, width: f64, height: f64, 
    radius: f64, roughness: f64, seed: i32
) -> Vec<(String, f64)> {
    let mut paths = Vec::new();
    
    if roughness <= 0.0 {
        // No roughness - return smooth path
        let path_data = generate_rounded_rect_path(x, y, width, height, radius);
        paths.push((path_data, 1.0));
        return paths;
    }
    
    // Generate corner points based on whether we have rounded corners
    let corner_points = if radius > 0.0 {
        generate_rounded_rect_points(x, y, width, height, radius)
    } else {
        vec![
            (x, y),
            (x + width, y),
            (x + width, y + height),
            (x, y + height),
        ]
    };
    
    let bowing = 1.0; // Default bowing value from rough.js
    let max_offset = 2.0 * roughness.sqrt(); // maxRandomnessOffset, scaled by roughness
    let preserve_vertices = roughness < 1.5;

    // Primary pass (underlay) - like rough.js linearPath with close=true
    let mut rng1 = LcgRng::new(seed);
    let mut segments1 = Vec::new();
    
    for i in 0..corner_points.len() {
        let (x1, y1) = corner_points[i];
        let (x2, y2) = corner_points[(i + 1) % corner_points.len()];
        segments1.push(generate_rough_line_segment(
            x1, y1, x2, y2,
            &mut rng1,
            roughness,
            bowing,
            max_offset,
            preserve_vertices,
            false, // not overlay
        ));
    }
    paths.push((segments1.join(" "), 1.0));
    
    // Secondary pass (overlay) - uses half offset and doesn't preserve vertices
    if roughness > 0.0 {
        let mut rng2 = LcgRng::new(seed.wrapping_add(1)); // Different seed for overlay
        let mut segments2 = Vec::new();
        
        for i in 0..corner_points.len() {
            let (x1, y1) = corner_points[i];
            let (x2, y2) = corner_points[(i + 1) % corner_points.len()];
            segments2.push(generate_rough_line_segment(
                x1, y1, x2, y2,
                &mut rng2,
                roughness,
                bowing,
                max_offset,
                false, // never preserve vertices on overlay
                true, // overlay mode
            ));
        }
        paths.push((segments2.join(" "), 0.85));
    }
    
    paths
}

fn render_element(el: &ExcalidrawElement, _viewbox: &ViewBox) -> String {
    if el.is_deleted {
        return String::new();
    }

    // Determine if we should render stroke
    let has_stroke = !el.stroke_color.is_empty() 
        && el.stroke_color != "transparent" 
        && el.stroke_width > 0.0;
    
    let stroke_color = if has_stroke {
        &el.stroke_color
    } else {
        "none"
    };
    
    // Determine if we should render fill
    let has_fill = !el.background_color.is_empty() 
        && el.background_color != "transparent";
    
    let background_color = if has_fill {
        &el.background_color
    } else {
        "none"
    };

    let opacity = el.opacity / 100.0;
    let transform = format!(
        "rotate({} {} {})",
        el.angle,
        el.x + el.width / 2.0,
        el.y + el.height / 2.0
    );
    
    let stroke_dasharray = get_stroke_dasharray(&el.stroke_style);
    let dasharray_attr = if stroke_dasharray != "none" {
        format!(r#" stroke-dasharray="{stroke_dasharray}""#)
    } else {
        String::new()
    };

    match el.element_type.as_str() {
        "rectangle" => {
            let fill_style = if el.fill_style.is_empty() {
                "solid"
            } else {
                el.fill_style.as_str()
            };
            
            // Calculate corner radius using Excalidraw's algorithm
            let radius = if el.roundness.is_some() {
                get_corner_radius(el.width.min(el.height), el)
            } else {
                0.0
            };
            
            // For non-solid fills, we need two paths: one for the pattern, one for the stroke
            if fill_style != "solid" && has_fill {
                let pattern_path = if fill_style == "hachure" {
                    generate_hachure_pattern(el.x, el.y, el.width, el.height, el.angle)
                } else {
                    // TODO: implement cross-hatch, zigzag patterns
                    String::new()
                };
                
                // Pattern path (using backgroundColor as stroke color)
                let pattern_svg = if !pattern_path.is_empty() {
                    format!(
                        r#"<path d="{}" fill="none" stroke="{}" stroke-width="1" opacity="{}" transform="{}"/>"#,
                        pattern_path, &el.background_color, opacity, transform
                    )
                } else {
                    String::new()
                };
                
                // Border path (stroke only)
                let border_svg = if has_stroke {
                    if radius > 0.0 {
                        let path_data = generate_rounded_rect_path(el.x, el.y, el.width, el.height, radius);
                        format!(
                            r#"<path d="{}" fill="none" stroke="{}" stroke-width="{}" opacity="{}" stroke-linecap="round"{} transform="{}"/>"#,
                            path_data, stroke_color, el.stroke_width, opacity, dasharray_attr, transform
                        )
                    } else {
                        format!(
                            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="{}" stroke-width="{}" opacity="{}" stroke-linecap="round"{} transform="{}"/>"#,
                            el.x, el.y, el.width, el.height, stroke_color, el.stroke_width, opacity, dasharray_attr, transform
                        )
                    }
                } else {
                    String::new()
                };
                
                format!("{pattern_svg}\n{border_svg}")
            } else {
                // Solid fill or no fill - use single path/rect
                let has_roughness = el.roughness > 0.0;
                
                if has_roughness {
                    // Separate fill and stroke like rough.js does
                    let mut svg_parts = Vec::new();
                    
                    // Fill path (if has fill) - single smooth path with no stroke
                    if has_fill {
                        let fill_path = if radius > 0.0 {
                            generate_rounded_rect_path(el.x, el.y, el.width, el.height, radius)
                        } else {
                            format!("M{},{} L{},{} L{},{} L{},{} Z",
                                el.x, el.y,
                                el.x + el.width, el.y,
                                el.x + el.width, el.y + el.height,
                                el.x, el.y + el.height
                            )
                        };
                        svg_parts.push(format!(
                            r#"<path d="{fill_path}" fill="{background_color}" stroke="none" opacity="{opacity}" transform="{transform}"/>"#
                        ));
                    }
                    
                    // Stroke paths (if has stroke) - multi-pass rough outline with no fill
                    if has_stroke {
                        let rough_paths = generate_rough_rect_paths(el.x, el.y, el.width, el.height, radius, el.roughness, el.seed);
                        for (path_data, path_opacity_multiplier) in rough_paths {
                            let combined_opacity = opacity * path_opacity_multiplier;
                            svg_parts.push(format!(
                                r#"<path d="{}" fill="none" stroke="{}" stroke-width="{}" opacity="{}" stroke-linecap="round"{} transform="{}"/>"#,
                                path_data, stroke_color, el.stroke_width, combined_opacity, dasharray_attr, transform
                            ));
                        }
                    }
                    
                    svg_parts.join("\n")
                } else if radius > 0.0 {
                    // Use smooth rounded path
                    let path_data = generate_rounded_rect_path(el.x, el.y, el.width, el.height, radius);
                    format!(
                        r#"<path d="{}" fill="{}" stroke="{}" stroke-width="{}" opacity="{}" stroke-linecap="round"{} transform="{}"/>"#,
                        path_data, background_color, stroke_color, el.stroke_width, opacity, dasharray_attr, transform
                    )
                } else {
                    // Use regular rect
                    format!(
                        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" stroke="{}" stroke-width="{}" opacity="{}" stroke-linecap="round"{} transform="{}"/>"#,
                        el.x, el.y, el.width, el.height, background_color, stroke_color, el.stroke_width, opacity, dasharray_attr, transform
                    )
                }
            }
        }
        "diamond" => {
            let points = [
                (el.x + el.width / 2.0, el.y),
                (el.x + el.width, el.y + el.height / 2.0),
                (el.x + el.width / 2.0, el.y + el.height),
                (el.x, el.y + el.height / 2.0),
            ];
            
            let has_roughness = el.roughness > 0.0;
            
            if has_roughness {
                // Separate fill and stroke like rough.js
                let mut svg_parts = Vec::new();
                
                // Fill path (if has fill) - single smooth polygon with no stroke
                if has_fill {
                    let points_str = points
                        .iter()
                        .map(|(x, y)| format!("{x},{y}"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    svg_parts.push(format!(
                        r#"<polygon points="{points_str}" fill="{background_color}" stroke="none" opacity="{opacity}" transform="{transform}"/>"#
                    ));
                }
                
                // Stroke paths (if has stroke) - multi-pass rough outline with no fill
                if has_stroke {
                    let rough_paths = generate_rough_polygon_paths(&points, el.roughness, el.seed);
                    for (path_data, path_opacity_multiplier) in rough_paths {
                        let combined_opacity = opacity * path_opacity_multiplier;
                        svg_parts.push(format!(
                            r#"<path d="{}" fill="none" stroke="{}" stroke-width="{}" opacity="{}" stroke-linecap="round" stroke-linejoin="round"{} transform="{}"/>"#,
                            path_data, stroke_color, el.stroke_width, combined_opacity, dasharray_attr, transform
                        ));
                    }
                }
                
                svg_parts.join("\n")
            } else {
                // Smooth polygon
                let points_str = points
                    .iter()
                    .map(|(x, y)| format!("{x},{y}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                format!(
                    r#"<polygon points="{}" fill="{}" stroke="{}" stroke-width="{}" opacity="{}" stroke-linecap="round" stroke-linejoin="round"{} transform="{}"/>"#,
                    points_str, background_color, stroke_color, el.stroke_width, opacity, dasharray_attr, transform
                )
            }
        }
        "ellipse" => {
            let cx = el.x + el.width / 2.0;
            let cy = el.y + el.height / 2.0;
            let rx = el.width / 2.0;
            let ry = el.height / 2.0;
            
            let has_roughness = el.roughness > 0.0;
            
            if has_roughness {
                // Separate fill and stroke like rough.js
                let mut svg_parts = Vec::new();
                
                // Fill path (if has fill) - single smooth ellipse with no stroke
                if has_fill {
                    svg_parts.push(format!(
                        r#"<ellipse cx="{cx}" cy="{cy}" rx="{rx}" ry="{ry}" fill="{background_color}" stroke="none" opacity="{opacity}" transform="{transform}"/>"#
                    ));
                }
                
                // Stroke paths (if has stroke) - multi-pass rough outline with no fill
                if has_stroke {
                    let rough_paths = generate_rough_ellipse_paths(cx, cy, rx, ry, el.roughness, el.seed);
                    for (path_data, path_opacity_multiplier) in rough_paths {
                        let combined_opacity = opacity * path_opacity_multiplier;
                        svg_parts.push(format!(
                            r#"<path d="{}" fill="none" stroke="{}" stroke-width="{}" opacity="{}" stroke-linecap="round" stroke-linejoin="round"{} transform="{}"/>"#,
                            path_data, stroke_color, el.stroke_width, combined_opacity, dasharray_attr, transform
                        ));
                    }
                }
                
                svg_parts.join("\n")
            } else {
                // Smooth ellipse
                format!(
                    r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" fill="{}" stroke="{}" stroke-width="{}" opacity="{}" stroke-linecap="round"{} transform="{}"/>"#,
                    cx, cy, rx, ry, background_color, stroke_color, el.stroke_width, opacity, dasharray_attr, transform
                )
            }
        }
        "line" | "arrow" => {
            if let Some(ref points) = el.points {
                if !points.is_empty() {
                    // Absolute points
                    let abs_points: Vec<(f64, f64)> = points.iter().map(|(px, py)| (el.x + px, el.y + py)).collect();

                    // When not elbowed, render as smoothed Catmull–Rom spline, else as polyline
                    let elbowed = el.elbowed.unwrap_or(false);
                    let path_data = if elbowed {
                        format!(
                            "M {}",
                            abs_points
                                .iter()
                                .map(|(x, y)| format!("{x},{y}"))
                                .collect::<Vec<_>>()
                                .join(" L ")
                        )
                    } else {
                        catmull_rom_path(&abs_points)
                    };

                    // Build optional arrowheads at start/end
                    let mut arrowheads_svg = String::new();

                    // Helper to convert shared arrowhead points to Vec<(f64, f64)> format
                    fn convert_arrowhead_points(arrowhead: &str, vals: Vec<f64>) -> Vec<(f64, f64)> {
                        match arrowhead {
                            "dot" | "circle" | "circle_outline" => {
                                if vals.len() >= 3 {
                                    vec![(vals[0], vals[1]), (vals[2], 0.0)]
                                } else {
                                    vec![]
                                }
                            }
                            "bar" => {
                                if vals.len() >= 4 {
                                    vec![(vals[0], vals[1]), (vals[2], vals[3])]
                                } else {
                                    vec![]
                                }
                            }
                            "arrow" | "triangle" | "triangle_outline" => {
                                if vals.len() >= 6 {
                                    vec![(vals[0], vals[1]), (vals[2], vals[3]), (vals[4], vals[5])]
                                } else {
                                    vec![]
                                }
                            }
                            "diamond" | "diamond_outline" => {
                                if vals.len() >= 8 {
                                    vec![(vals[0], vals[1]), (vals[2], vals[3]), (vals[4], vals[5]), (vals[6], vals[7])]
                                } else {
                                    vec![]
                                }
                            }
                            _ => vec![],
                        }
                    }

                    // Render arrowhead helper function
                    #[allow(clippy::too_many_arguments)]
                    fn render_arrowhead(
                        arrowhead_type: &str,
                        points_vec: Vec<(f64, f64)>,
                        stroke_color: &str,
                        background_color: &str,
                        stroke_width: f64,
                        opacity: f64,
                        transform: &str,
                    ) -> String {
                        if points_vec.is_empty() {
                            return String::new();
                        }

                        match arrowhead_type {
                            "dot" | "circle" | "circle_outline" => {
                                if points_vec.len() >= 2 {
                                    let (cx, cy) = points_vec[0];
                                    let (diameter, _) = points_vec[1];
                                    let fill = if arrowhead_type == "circle_outline" {
                                        background_color
                                    } else {
                                        stroke_color
                                    };
                                    format!(
                                        r#"<circle cx="{cx}" cy="{cy}" r="{}" fill="{fill}" stroke="{stroke_color}" stroke-width="{stroke_width}" opacity="{opacity}" transform="{transform}"/>"#,
                                        diameter / 2.0
                                    )
                                } else {
                                    String::new()
                                }
                            }
                            "bar" => {
                                if points_vec.len() >= 2 {
                                    let (x1, y1) = points_vec[0];
                                    let (x2, y2) = points_vec[1];
                                    format!(
                                        r#"<path d="M {x1} {y1} L {x2} {y2}" fill="none" stroke="{stroke_color}" stroke-width="{stroke_width}" opacity="{opacity}" transform="{transform}" stroke-linecap="round"/>"#
                                    )
                                } else {
                                    String::new()
                                }
                            }
                            "arrow" => {
                                if points_vec.len() >= 3 {
                                    let (tip_x, tip_y) = points_vec[0];
                                    let (x3, y3) = points_vec[1];
                                    let (x4, y4) = points_vec[2];
                                    format!(
                                        r#"<path d="M {x3} {y3} L {tip_x} {tip_y}" fill="none" stroke="{stroke_color}" stroke-width="{stroke_width}" opacity="{opacity}" transform="{transform}" stroke-linecap="round"/>"#
                                    ) + "\n" + &format!(
                                        r#"<path d="M {x4} {y4} L {tip_x} {tip_y}" fill="none" stroke="{stroke_color}" stroke-width="{stroke_width}" opacity="{opacity}" transform="{transform}" stroke-linecap="round"/>"#
                                    )
                                } else {
                                    String::new()
                                }
                            }
                            "triangle" | "triangle_outline" => {
                                if points_vec.len() >= 3 {
                                    let fill = if arrowhead_type == "triangle_outline" {
                                        background_color
                                    } else {
                                        stroke_color
                                    };
                                    let path_points = points_vec.iter()
                                        .map(|(x, y)| format!("{x},{y}"))
                                        .collect::<Vec<_>>()
                                        .join(" ");
                                    format!(
                                        r#"<polygon points="{path_points}" fill="{fill}" stroke="{stroke_color}" stroke-width="{stroke_width}" opacity="{opacity}" transform="{transform}"/>"#
                                    )
                                } else {
                                    String::new()
                                }
                            }
                            "diamond" | "diamond_outline" => {
                                if points_vec.len() >= 4 {
                                    let fill = if arrowhead_type == "diamond_outline" {
                                        background_color
                                    } else {
                                        stroke_color
                                    };
                                    let path_points = points_vec.iter()
                                        .map(|(x, y)| format!("{x},{y}"))
                                        .collect::<Vec<_>>()
                                        .join(" ");
                                    format!(
                                        r#"<polygon points="{path_points}" fill="{fill}" stroke="{stroke_color}" stroke-width="{stroke_width}" opacity="{opacity}" transform="{transform}"/>"#
                                    )
                                } else {
                                    String::new()
                                }
                            }
                            _ => String::new(),
                        }
                    }

                    // END arrowhead
                    if (el.end_arrowhead.is_some() || el.end_arrow_type.is_some()) && points.len() >= 2 {
                        let arrowhead_type = el.end_arrowhead.as_deref()
                            .or(el.end_arrow_type.as_deref())
                            .unwrap_or("arrow");
                        
                        let (last_rel_x, last_rel_y) = points[points.len() - 1];
                        let (prev_rel_x, prev_rel_y) = points[points.len() - 2];
                        let tip_x = el.x + last_rel_x;
                        let tip_y = el.y + last_rel_y;
                        let tail_x = el.x + prev_rel_x;
                        let tail_y = el.y + prev_rel_y;
                        
                        let segment_length = ((tip_x - tail_x).powi(2) + (tip_y - tail_y).powi(2)).sqrt();
                        let pts_vals = calc_arrowhead_points(tail_x, tail_y, tip_x, tip_y, arrowhead_type, el.stroke_width, segment_length);
                        let pts = convert_arrowhead_points(arrowhead_type, pts_vals);
                        
                        let arrowhead_svg = render_arrowhead(
                            arrowhead_type,
                            pts.clone(),
                            stroke_color,
                            background_color,
                            el.stroke_width,
                            opacity,
                            &transform,
                        );
                        arrowheads_svg.push_str(&arrowhead_svg);

                        // Rough imperfect second pass for arrowhead if roughness > 0
                        if el.roughness > 0.0 && arrowhead_type != "dot" {
                            let mut rng = LcgRng::new(el.seed);
                            let jitter = (0.6 + 0.2 * el.stroke_width) * el.roughness.max(0.0);
                            let jx = rng.range(-jitter, jitter);
                            let jy = rng.range(-jitter, jitter);
                            let pts_rough_vals = calc_arrowhead_points(
                                tail_x + jx, tail_y + jy, 
                                tip_x + jx, tip_y + jy, 
                                arrowhead_type, 
                                el.stroke_width * rng.range(0.95, 1.05), 
                                segment_length
                            );
                            let pts_rough = convert_arrowhead_points(arrowhead_type, pts_rough_vals);
                            let opacity2 = (opacity * 0.9).min(1.0);
                            arrowheads_svg.push('\n');
                            arrowheads_svg.push_str(&render_arrowhead(
                                arrowhead_type,
                                pts_rough,
                                stroke_color,
                                background_color,
                                el.stroke_width,
                                opacity2,
                                &transform,
                            ));
                        }
                    }

                    // START arrowhead
                    if (el.start_arrowhead.is_some() || el.start_arrow_type.is_some()) && points.len() >= 2 {
                        let arrowhead_type = el.start_arrowhead.as_deref()
                            .or(el.start_arrow_type.as_deref())
                            .unwrap_or("arrow");
                        
                        let (first_rel_x, first_rel_y) = points[0];
                        let (second_rel_x, second_rel_y) = points[1];
                        let tip_x = el.x + first_rel_x;
                        let tip_y = el.y + first_rel_y;
                        let tail_x = el.x + second_rel_x;
                        let tail_y = el.y + second_rel_y;
                        
                        let segment_length = ((tip_x - tail_x).powi(2) + (tip_y - tail_y).powi(2)).sqrt();
                        let pts_vals = calc_arrowhead_points(tail_x, tail_y, tip_x, tip_y, arrowhead_type, el.stroke_width, segment_length);
                        let pts = convert_arrowhead_points(arrowhead_type, pts_vals);
                        
                        arrowheads_svg.push('\n');
                        arrowheads_svg.push_str(&render_arrowhead(
                            arrowhead_type,
                            pts.clone(),
                            stroke_color,
                            background_color,
                            el.stroke_width,
                            opacity,
                            &transform,
                        ));

                        // Rough imperfect second pass for start arrowhead if roughness > 0
                        if el.roughness > 0.0 && arrowhead_type != "dot" {
                            let mut rng = LcgRng::new(el.seed ^ 0xABCDEF);
                            let jitter = (0.6 + 0.2 * el.stroke_width) * el.roughness.max(0.0);
                            let jx = rng.range(-jitter, jitter);
                            let jy = rng.range(-jitter, jitter);
                            let pts_rough_vals = calc_arrowhead_points(
                                tail_x + jx, tail_y + jy, 
                                tip_x + jx, tip_y + jy, 
                                arrowhead_type, 
                                el.stroke_width * rng.range(0.95, 1.05), 
                                segment_length
                            );
                            let pts_rough = convert_arrowhead_points(arrowhead_type, pts_rough_vals);
                            let opacity2 = (opacity * 0.9).min(1.0);
                            arrowheads_svg.push('\n');
                            arrowheads_svg.push_str(&render_arrowhead(
                                arrowhead_type,
                                pts_rough,
                                stroke_color,
                                background_color,
                                el.stroke_width,
                                opacity2,
                                &transform,
                            ));
                        }
                    }

                    // Main shaft path with rounded caps/joins
                    let shaft_svg = format!(
                        r#"<path d="{}" fill="none" stroke="{}" stroke-width="{}" opacity="{}"{} transform="{}" stroke-linecap="round" stroke-linejoin="round"/>"#,
                        path_data, stroke_color, el.stroke_width, opacity, dasharray_attr, transform
                    );

                    // Rough multi-pass rendering for shaft if roughness > 0
                    let mut rough_passes = vec![shaft_svg];
                    
                    if el.roughness > 0.0 {
                        let mut rng = LcgRng::new(el.seed);
                        let amplitude = (1.2 + 0.3 * el.stroke_width) * el.roughness.max(0.0);
                        
                        // Secondary pass - main jitter
                        let jittered = jitter_polyline(&abs_points, &mut rng, amplitude);
                        let jitter_path = if elbowed {
                            format!(
                                "M {}",
                                jittered
                                    .iter()
                                    .map(|(x, y)| format!("{x},{y}"))
                                    .collect::<Vec<_>>()
                                    .join(" L ")
                            )
                        } else {
                            catmull_rom_path(&jittered)
                        };
                        let opacity2 = (opacity * 0.85).min(1.0);
                        rough_passes.push(format!(
                            r#"<path d="{}" fill="none" stroke="{}" stroke-width="{}" opacity="{}"{} transform="{}" stroke-linecap="round" stroke-linejoin="round"/>"#,
                            jitter_path, stroke_color, el.stroke_width, opacity2, dasharray_attr, transform
                        ));
                        
                        // Tertiary pass for high roughness
                        if el.roughness > 1.0 {
                            let mut rng3 = LcgRng::new(el.seed.wrapping_add(0x55555555_u32 as i32));
                            let amplitude3 = amplitude * 0.6;
                            let jittered3 = jitter_polyline(&abs_points, &mut rng3, amplitude3);
                            let jitter_path3 = if elbowed {
                                format!(
                                    "M {}",
                                    jittered3
                                        .iter()
                                        .map(|(x, y)| format!("{x},{y}"))
                                        .collect::<Vec<_>>()
                                        .join(" L ")
                                )
                            } else {
                                catmull_rom_path(&jittered3)
                            };
                            let opacity3 = (opacity * 0.7).min(1.0);
                            rough_passes.push(format!(
                                r#"<path d="{}" fill="none" stroke="{}" stroke-width="{}" opacity="{}"{} transform="{}" stroke-linecap="round" stroke-linejoin="round"/>"#,
                                jitter_path3, stroke_color, el.stroke_width, opacity3, dasharray_attr, transform
                            ));
                        }
                    }

                    let all_shafts = rough_passes.join("\n");
                    
                    return if arrowheads_svg.is_empty() {
                        all_shafts
                    } else {
                        format!("{all_shafts}\n{arrowheads_svg}")
                    };
                }
            }
            String::new()
        }
        "text" => {
            let font_size = el.font_size.unwrap_or(16.0);
            let text = el.text.as_deref().unwrap_or("");
            let font_family = get_font_family(el.font_family);
            let line_height_px = get_line_height(font_size, el.line_height);
            
            // Handle text alignment - calculate absolute x position
            let x_pos = if el.text_align.as_deref() == Some("center") {
                el.x + el.width / 2.0
            } else if el.text_align.as_deref() == Some("right") {
                el.x + el.width
            } else {
                el.x
            };
            
            let alignment_anchor = get_text_anchor(el.text_align.as_deref());
            
            // Calculate vertical offset based on font metrics
            let vertical_offset = get_vertical_offset(None, font_size);
            
            // Split text into lines
            let lines: Vec<&str> = text.split('\n').collect();
            
            // Create tspan elements for each line
            let tspan_elements: Vec<String> = lines.iter().enumerate().map(|(i, line)| {
                let y_pos = el.y + (i as f64) * line_height_px + vertical_offset;
                format!(
                    r#"<tspan x="{}" y="{}" style="white-space: pre;">{}</tspan>"#,
                    x_pos, y_pos, escape_xml(line)
                )
            }).collect();
            
            format!(
                r#"<text font-size="{}" font-family="{}" fill="{}" opacity="{}" text-anchor="{}" dominant-baseline="alphabetic" transform="{}">{}</text>"#,
                font_size,
                font_family,
                stroke_color,
                opacity,
                alignment_anchor,
                transform,
                tspan_elements.join("\n")
            )
        }
        _ => String::new(),
    }
}

fn escape_xml(s: &str) -> String {
    s.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\"", "&quot;")
        .replace("'", "&apos;")
}

/// Generate hachure pattern (diagonal lines) for a rectangle
fn generate_hachure_pattern(x: f64, y: f64, width: f64, height: f64, angle: f64) -> String {
    let gap = 4.0; // spacing between hachure lines
    let hachure_angle = -45.0; // diagonal lines at -45 degrees
    
    // Calculate the angle in radians accounting for both shape rotation and hachure angle
    let rad = (angle + hachure_angle).to_radians();
    let cos_angle = rad.cos();
    let sin_angle = rad.sin();
    
    // Calculate bounding box diagonal to determine how many lines we need
    let diagonal = (width.powi(2) + height.powi(2)).sqrt();
    let num_lines = (diagonal / gap).ceil() as i32;
    
    let mut lines = Vec::new();
    
    // Generate lines from top-left to bottom-right direction
    for i in -num_lines..=num_lines {
        let offset = i as f64 * gap;
        
        // Calculate line endpoints in rotated space
        // Start from center of rectangle and offset perpendicular to hachure direction
        let center_x = x + width / 2.0;
        let center_y = y + height / 2.0;
        
        // Perpendicular offset direction
        let perp_x = -sin_angle * offset;
        let perp_y = cos_angle * offset;
        
        // Line direction (along the hachure angle)
        let line_x = cos_angle * diagonal;
        let line_y = sin_angle * diagonal;
        
        // Line endpoints
        let x1 = center_x + perp_x - line_x;
        let y1 = center_y + perp_y - line_y;
        let x2 = center_x + perp_x + line_x;
        let y2 = center_y + perp_y + line_y;
        
        // Clip line to rectangle bounds
        if let Some((cx1, cy1, cx2, cy2)) = clip_line_to_rect(x1, y1, x2, y2, x, y, width, height) {
            lines.push(format!("M{cx1:.2},{cy1:.2} L{cx2:.2},{cy2:.2}"));
        }
    }
    
    lines.join(" ")
}

/// Clip a line to a rectangle using Cohen-Sutherland algorithm
#[allow(clippy::too_many_arguments)]
fn clip_line_to_rect(x1: f64, y1: f64, x2: f64, y2: f64, rx: f64, ry: f64, rw: f64, rh: f64) -> Option<(f64, f64, f64, f64)> {
    const INSIDE: u8 = 0; // 0000
    const LEFT: u8 = 1;   // 0001
    const RIGHT: u8 = 2;  // 0010
    const BOTTOM: u8 = 4; // 0100
    const TOP: u8 = 8;    // 1000
    
    fn compute_code(x: f64, y: f64, rx: f64, ry: f64, rw: f64, rh: f64) -> u8 {
        let mut code = INSIDE;
        if x < rx { code |= LEFT; }
        else if x > rx + rw { code |= RIGHT; }
        if y < ry { code |= TOP; }
        else if y > ry + rh { code |= BOTTOM; }
        code
    }
    
    let mut x1 = x1;
    let mut y1 = y1;
    let mut x2 = x2;
    let mut y2 = y2;
    
    let mut code1 = compute_code(x1, y1, rx, ry, rw, rh);
    let mut code2 = compute_code(x2, y2, rx, ry, rw, rh);
    
    loop {
        if (code1 | code2) == 0 {
            // Both points inside
            return Some((x1, y1, x2, y2));
        } else if (code1 & code2) != 0 {
            // Both points outside on same side
            return None;
        } else {
            // Line needs clipping
            let code_out = if code1 != 0 { code1 } else { code2 };
            
            let (x, y) = if (code_out & TOP) != 0 {
                let x = x1 + (x2 - x1) * (ry - y1) / (y2 - y1);
                (x, ry)
            } else if (code_out & BOTTOM) != 0 {
                let x = x1 + (x2 - x1) * (ry + rh - y1) / (y2 - y1);
                (x, ry + rh)
            } else if (code_out & RIGHT) != 0 {
                let y = y1 + (y2 - y1) * (rx + rw - x1) / (x2 - x1);
                (rx + rw, y)
            } else { // LEFT
                let y = y1 + (y2 - y1) * (rx - x1) / (x2 - x1);
                (rx, y)
            };
            
            if code_out == code1 {
                x1 = x;
                y1 = y;
                code1 = compute_code(x1, y1, rx, ry, rw, rh);
            } else {
                x2 = x;
                y2 = y;
                code2 = compute_code(x2, y2, rx, ry, rw, rh);
            }
        }
    }
}

pub fn generate_svg(data: &ExcalidrawData, background: Option<(u8,u8,u8,u8)>) -> String {
    let viewbox = calculate_viewbox(&data.elements);

    let elements_svg = data
        .elements
        .iter()
        .map(|el| render_element(el, &viewbox))
        .collect::<Vec<_>>()
        .join("\n");

    let fill_color = "#000000";

    // Optional background rect
    let bg_rect = if let Some((r,g,b,a)) = background {
        if a == 0 { String::new() } else {
            let hex = format!("#{r:02x}{g:02x}{b:02x}");
            let opacity = (a as f64) / 255.0;
            format!(
                "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" fill-opacity=\"{:.4}\"/>\n",
                viewbox.min_x, viewbox.min_y, viewbox.width, viewbox.height, hex, opacity
            )
        }
    } else {
        String::new()
    };

    format!(
        "<svg viewBox=\"{} {} {} {}\" xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">\n  <defs>\n    <marker id=\"arrowhead\" markerWidth=\"10\" markerHeight=\"10\" refX=\"9\" refY=\"3\" orient=\"auto\">\n      <polygon points=\"0 0, 10 3, 0 6\" fill=\"{}\"/>\n    </marker>\n  </defs>\n{}  {}\n</svg>",
        viewbox.min_x, viewbox.min_y, viewbox.width, viewbox.height, fill_color, bg_rect, elements_svg
    )
}
