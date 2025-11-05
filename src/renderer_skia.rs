use crate::models::{ExcalidrawData, ExcalidrawElement as Element, ViewBox};
use crate::converter::{EXCALIFONT_REGULAR, LIBERATION_SANS_REGULAR, CASCADIA_CODE};
use crate::utils::save_png_with_quality;
use anyhow::Result;
use euclid::default::Point2D;
use palette::Srgba;
use parley::{FontContext, LayoutContext, StyleProperty};
use rough_tiny_skia::SkiaGenerator;
use roughr::core::{FillStyle, OptionsBuilder};
use skrifa::{GlyphId, MetadataProvider, OutlineGlyph, instance::{LocationRef, NormalizedCoord, Size}, outline::{DrawSettings, OutlinePen}, raw::FontRef as ReadFontsRef};
use tiny_skia::*;

pub fn calculate_viewbox(elements: &[Element]) -> ViewBox {
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

/// Parse hex color string to RGBA components
fn parse_color(color_str: &str) -> (u8, u8, u8, u8) {
    if color_str.starts_with('#') && color_str.len() == 7 {
        let r = u8::from_str_radix(&color_str[1..3], 16).unwrap_or(0);
        let g = u8::from_str_radix(&color_str[3..5], 16).unwrap_or(0);
        let b = u8::from_str_radix(&color_str[5..7], 16).unwrap_or(0);
        (r, g, b, 255)
    } else if color_str == "transparent" || color_str.is_empty() {
        (0, 0, 0, 0)
    } else {
        // Default to black
        (0, 0, 0, 255)
    }
}

// Excalidraw-accurate arrowhead sizing
fn exca_get_arrowhead_size(arrowhead: &str) -> f32 {
    match arrowhead {
        "arrow" => 25.0,
        "diamond" | "diamond_outline" => 12.0,
        "crowfoot_many" | "crowfoot_one" | "crowfoot_one_or_many" => 20.0,
        _ => 15.0,
    }
}

// Excalidraw-accurate arrowhead angles (degrees)
fn exca_get_arrowhead_angle(arrowhead: &str) -> f32 {
    match arrowhead {
        "bar" => 90.0,
        "arrow" => 20.0,
        _ => 25.0,
    }
}

/// Compute stroke dash pattern based on Excalidraw's strokeStyle and strokeWidth
/// Matches packages/element/src/shape.ts getDashArrayDashed/Dotted
fn exca_stroke_dash(stroke_style: &str, stroke_width: f32) -> Option<Vec<f32>> {
    match stroke_style {
        "dashed" => {
            // [8, 8 + strokeWidth]
            Some(vec![8.0, 8.0 + stroke_width.max(0.0)])
        }
        "dotted" => {
            // [1.5, 6 + strokeWidth]
            Some(vec![1.5, 6.0 + stroke_width.max(0.0)])
        }
        _ => None,
    }
}

/// Dotted dash pattern for arrow caps uses strokeWidth-1 (see shape.ts)
fn exca_dotted_cap_dash(stroke_width: f32) -> Vec<f32> {
    let adj = (stroke_width - 1.0).max(0.0);
    vec![1.5, 6.0 + adj]
}

// Build Catmull–Rom cubic segments in absolute coords
#[allow(clippy::type_complexity)]
fn catmull_rom_cubics_abs(points: &[(f64, f64)], x: f32, y: f32) -> Vec<((f32, f32), (f32, f32), (f32, f32), (f32, f32))> {
    let n = points.len();
    if n < 2 { return vec![]; }
    let abs: Vec<(f32,f32)> = points.iter().map(|(px,py)| (x + *px as f32, y + *py as f32)).collect();
    if n == 2 {
        return vec![(abs[0], abs[0], abs[1], abs[1])];
    }
    let mut segs = Vec::new();
    let tension: f32 = 0.5;
    let get = |i: isize| -> (f32,f32) {
        let nn = abs.len() as isize;
        let idx = if i < 0 { 0 } else if i >= nn { nn - 1 } else { i } as usize;
        abs[idx]
    };
    for i in 0..(abs.len() - 1) {
        let p0 = get(i as isize - 1);
        let p1 = get(i as isize);
        let p2 = get(i as isize + 1);
        let p3 = get(i as isize + 2);
        let t1x = (p2.0 - p0.0) * tension;
        let t1y = (p2.1 - p0.1) * tension;
        let t2x = (p3.0 - p1.0) * tension;
        let t2y = (p3.1 - p1.1) * tension;
        let cp1 = (p1.0 + t1x / 3.0, p1.1 + t1y / 3.0);
        let cp2 = (p2.0 - t2x / 3.0, p2.1 - t2y / 3.0);
        segs.push((p1, cp1, cp2, p2));
    }
    segs
}

fn cubic_point(p0:(f32,f32), p1:(f32,f32), p2:(f32,f32), p3:(f32,f32), t:f32) -> (f32,f32) {
    let u = 1.0 - t;
    let u2 = u*u; let u3 = u2*u; let t2 = t*t; let t3 = t2*t;
    let x = u3*p0.0 + 3.0*u2*t*p1.0 + 3.0*u*t2*p2.0 + t3*p3.0;
    let y = u3*p0.1 + 3.0*u2*t*p1.1 + 3.0*u*t2*p2.1 + t3*p3.1;
    (x,y)
}

fn rotate_point(px:f32, py:f32, cx:f32, cy:f32, angle_rad:f32) -> (f32,f32) {
    let dx = px - cx; let dy = py - cy;
    let ca = angle_rad.cos(); let sa = angle_rad.sin();
    (cx + dx*ca - dy*sa, cy + dx*sa + dy*ca)
}

// Compute arrowhead points as per Excalidraw
fn exca_arrowhead_points(
    points: &[(f64,f64)],
    x: f32,
    y: f32,
    stroke_width: f32,
    arrowhead: &str,
    position: &str, // "start" or "end"
) -> Option<Vec<f32>> {
    if points.is_empty() { return None; }
    let cubics = catmull_rom_cubics_abs(points, x, y);
    if cubics.is_empty() { return None; }

    let (p0,p1,p2,p3) = if position == "start" { cubics[0] } else { *cubics.last().unwrap() };
    // Tip
    let (x2,y2) = if position == "start" { p0 } else { p3 };
    // Point near tip
    let t = if position == "start" { 0.3 } else { 0.7 };
    let (x1,y1) = cubic_point(p0,p1,p2,p3,t);
    let dx = x2 - x1; let dy = y2 - y1; let dist = (dx*dx + dy*dy).sqrt();
    if dist < 1e-3 { return None; }
    let nx = dx / dist; let ny = dy / dist;

    let size = exca_get_arrowhead_size(arrowhead);
    // segment length from element points (local)
    let seg_len = if position == "end" {
        if points.len() >= 2 {
            let a = points[points.len()-1]; let b = points[points.len()-2];
            ((a.0-b.0).powi(2) + (a.1-b.1).powi(2)).sqrt() as f32
        } else { 0.0 }
    } else if points.len() >= 2 {
        let a = points[0]; let b = points[1];
        ((a.0-b.0).powi(2) + (a.1-b.1).powi(2)).sqrt() as f32
    } else { 0.0 };
    let length_multiplier = if arrowhead == "diamond" || arrowhead == "diamond_outline" { 0.25 } else { 0.5 };
    let min_size = size.min(seg_len * length_multiplier);
    let xs = x2 - nx * min_size; let ys = y2 - ny * min_size;

    match arrowhead {
        "dot" | "circle" | "circle_outline" => {
            let diameter = ((ys - y2).powi(2) + (xs - x2).powi(2)).sqrt() + stroke_width - 2.0;
            Some(vec![x2, y2, diameter])
        }
        _ => {
            let angle_deg = exca_get_arrowhead_angle(arrowhead);
            let angle = angle_deg.to_radians();

            if arrowhead == "crowfoot_many" || arrowhead == "crowfoot_one_or_many" {
                // swap (xs,ys) with (x2,y2) and rotate around (xs,ys)
                let (x3,y3) = rotate_point(x2,y2,xs,ys,-angle);
                let (x4,y4) = rotate_point(x2,y2,xs,ys, angle);
                return Some(vec![xs,ys,x3,y3,x4,y4]);
            }

            let (x3,y3) = rotate_point(xs,ys,x2,y2,-angle);
            let (x4,y4) = rotate_point(xs,ys,x2,y2, angle);

            if arrowhead == "diamond" || arrowhead == "diamond_outline" {
                // Opposite point along shaft
                let ox = x2 - nx * min_size * 2.0;
                let oy = y2 - ny * min_size * 2.0;
                return Some(vec![x2,y2,x3,y3,ox,oy,x4,y4]);
            }

            // default/triangle/bar/arrow
            Some(vec![x2,y2,x3,y3,x4,y4])
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_arrowhead_ex(
    pixmap: &mut PixmapMut,
    points: &[(f64,f64)],
    x: f32,
    y: f32,
    stroke_rgba: (u8,u8,u8,u8),
    stroke_width: f32,
    arrowhead: &str,
    position: &str,
    cap_gen: &SkiaGenerator,
) {
    if let Some(vals) = exca_arrowhead_points(points, x, y, stroke_width, arrowhead, position) {
        match arrowhead {
            "dot" | "circle" | "circle_outline" => {
                let cx = vals[0]; let cy = vals[1]; let diameter = vals[2];
                // Render solid/outline circle with tiny-skia
                let mut path_builder = PathBuilder::new();
                path_builder.push_circle(cx, cy, diameter / 2.0);
                if let Some(path) = path_builder.finish() {
                    let mut paint = Paint::default();
                    // Fill color: outline => white background, else stroke color
                    let (fill_r,fill_g,fill_b,fill_a) = if arrowhead == "circle_outline" { (255,255,255,255) } else { stroke_rgba };
                    paint.set_color_rgba8(fill_r,fill_g,fill_b,fill_a);
                    pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
                    // Stroke outline
                    let mut spaint = Paint::default(); spaint.set_color_rgba8(stroke_rgba.0, stroke_rgba.1, stroke_rgba.2, stroke_rgba.3);
                    let stroke = Stroke { width: stroke_width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
                    pixmap.stroke_path(&path, &spaint, &stroke, Transform::identity(), None);
                }
            }
            "triangle" | "triangle_outline" => {
                let x0=vals[0]; let y0=vals[1]; let x1=vals[2]; let y1=vals[3]; let x2p=vals[4]; let y2p=vals[5];
                let mut pb = PathBuilder::new(); pb.move_to(x0,y0); pb.line_to(x1,y1); pb.line_to(x2p,y2p); pb.close();
                if let Some(path) = pb.finish() {
                    // Fill
                    let (fr,fg,fb,fa) = if arrowhead.ends_with("_outline") { (255,255,255,255) } else { stroke_rgba };
                    let mut fp = Paint::default(); fp.set_color_rgba8(fr,fg,fb,fa);
                    pixmap.fill_path(&path, &fp, FillRule::Winding, Transform::identity(), None);
                    // Stroke
                    let mut sp = Paint::default(); sp.set_color_rgba8(stroke_rgba.0, stroke_rgba.1, stroke_rgba.2, stroke_rgba.3);
                    let st = Stroke { width: stroke_width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
                    pixmap.stroke_path(&path, &sp, &st, Transform::identity(), None);
                }
            }
            "diamond" | "diamond_outline" => {
                let x0=vals[0]; let y0=vals[1]; let x1=vals[2]; let y1=vals[3]; let ox=vals[4]; let oy=vals[5]; let x2p=vals[6]; let y2p=vals[7];
                let mut pb = PathBuilder::new(); pb.move_to(x0,y0); pb.line_to(x1,y1); pb.line_to(ox,oy); pb.line_to(x2p,y2p); pb.close();
                if let Some(path) = pb.finish() {
                    let (fr,fg,fb,fa) = if arrowhead.ends_with("_outline") { (255,255,255,255) } else { stroke_rgba };
                    let mut fp = Paint::default(); fp.set_color_rgba8(fr,fg,fb,fa);
                    pixmap.fill_path(&path, &fp, FillRule::Winding, Transform::identity(), None);
                    let mut sp = Paint::default(); sp.set_color_rgba8(stroke_rgba.0, stroke_rgba.1, stroke_rgba.2, stroke_rgba.3);
                    let st = Stroke { width: stroke_width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
                    pixmap.stroke_path(&path, &sp, &st, Transform::identity(), None);
                }
            }
            "crowfoot_one" => {
                // vals: [x2,y2,x3,y3,x4,y4] per getArrowheadPoints, but here we use x3,y3,x4,y4 line
                let x3=vals[2]; let y3=vals[3]; let x4=vals[4]; let y4=vals[5];
                let line = cap_gen.line::<f32>(x3,y3,x4,y4);
                line.draw(pixmap);
            }
            "bar" => {
                // Draw only the perpendicular bar line x3,y3 -> x4,y4
                let x3=vals[2]; let y3=vals[3]; let x4=vals[4]; let y4=vals[5];
                let line = cap_gen.line::<f32>(x3,y3,x4,y4);
                line.draw(pixmap);
            }
            _ => {
                // default/arrow/bar/crowfoot_many/one_or_many -> two lines to tip
                let x2=vals[0]; let y2=vals[1]; let x3=vals[2]; let y3=vals[3]; let x4=vals[4]; let y4=vals[5];
                let l1 = cap_gen.line::<f32>(x3,y3,x2,y2); l1.draw(pixmap);
                let l2 = cap_gen.line::<f32>(x4,y4,x2,y2); l2.draw(pixmap);
                // extra for crowfoot_one_or_many: add crowfoot_one bar
                if arrowhead == "crowfoot_one_or_many" {
                    if let Some(bar) = exca_arrowhead_points(points,x,y,stroke_width,"crowfoot_one",position) {
                        let bx3=bar[2]; let by3=bar[3]; let bx4=bar[4]; let by4=bar[5];
                        let bl = cap_gen.line::<f32>(bx3,by3,bx4,by4); bl.draw(pixmap);
                    }
                }
            }
        }
    }
}

// Excalidraw roundness constants/types for rectangles
const DEFAULT_PROPORTIONAL_RADIUS: f32 = 0.25;
const DEFAULT_ADAPTIVE_RADIUS: f32 = 32.0;
const ROUNDNESS_LEGACY: i32 = 1;
const ROUNDNESS_PROPORTIONAL_RADIUS: i32 = 2;
const ROUNDNESS_ADAPTIVE_RADIUS: i32 = 3;

/// Calculate corner radius based on Excalidraw's roundness algorithm
fn get_corner_radius(size: f32, element: &Element) -> f32 {
    if let Some(ref roundness) = element.roundness {
        match roundness.roundness_type {
            ROUNDNESS_PROPORTIONAL_RADIUS | ROUNDNESS_LEGACY => {
                return size * DEFAULT_PROPORTIONAL_RADIUS;
            }
            ROUNDNESS_ADAPTIVE_RADIUS => {
                let fixed_radius_size = roundness.value.unwrap_or(DEFAULT_ADAPTIVE_RADIUS as f64) as f32;
                let cutoff_size = fixed_radius_size / DEFAULT_PROPORTIONAL_RADIUS;
                if size <= cutoff_size {
                    return size * DEFAULT_PROPORTIONAL_RADIUS;
                }
                return fixed_radius_size;
            }
            _ => return 0.0,
        }
    }
    0.0
}

fn heading_for_point_is_horizontal(p: (f32,f32), prev: (f32,f32)) -> bool {
    (p.0 - prev.0).abs() >= (p.1 - prev.1).abs()
}

fn dist2d(a:(f32,f32), b:(f32,f32)) -> f32 { ((a.0-b.0).powi(2) + (a.1-b.1).powi(2)).sqrt() }

/// Build Catmull–Rom cubic path d-string (M + C segments)
fn build_catmull_rom_cubic_path(points: &[(f64,f64)], x:f32, y:f32) -> Option<String> {
    let segs = catmull_rom_cubics_abs(points, x, y);
    if segs.is_empty() { return None; }
    let mut d = String::new();
    d.push_str(&format!("M {} {}", segs[0].0 .0, segs[0].0 .1));
    for (_, c1, c2, p3) in segs {
        d.push_str(&format!(" C {} {}, {} {}, {} {}", c1.0, c1.1, c2.0, c2.1, p3.0, p3.1));
    }
    Some(d)
}

/// Build elbow arrow path with rounded corners, converting Q to cubic C segments
fn build_elbow_arrow_cubic_path(points:&[(f64,f64)], x:f32, y:f32, max_corner:f32) -> Option<String> {
    if points.len() < 2 { return None; }
    if points.len() == 2 {
        let start = (x + points[0].0 as f32, y + points[0].1 as f32);
        let end = (x + points[1].0 as f32, y + points[1].1 as f32);
        let d = format!("M {} {} L {} {}", start.0, start.1, end.0, end.1);
        return Some(d);
    }
    // Build sub-commands: for each middle point, push L, Q control, Q end
    let mut sub: Vec<(f32,f32)> = Vec::new();
    for i in 1..(points.len() - 1) {
        let prev = (points[i - 1].0 as f32, points[i - 1].1 as f32);
        let curr = (points[i].0 as f32, points[i].1 as f32);
        let next = (points[i + 1].0 as f32, points[i + 1].1 as f32);
        let prev_is_h = heading_for_point_is_horizontal(curr, prev);
        let next_is_h = heading_for_point_is_horizontal(next, curr);
        let corner = max_corner.min(dist2d(curr,next) * 0.5).min(dist2d(prev,curr) * 0.5);

        // last point before corner
        if prev_is_h {
            if prev.0 < curr.0 { sub.push((curr.0 - corner, curr.1)); } else { sub.push((curr.0 + corner, curr.1)); }
        } else if prev.1 < curr.1 { sub.push((curr.0, curr.1 - corner)); } else { sub.push((curr.0, curr.1 + corner)); }

        // corner control point
        sub.push((curr.0, curr.1));

        // next segment start after the corner
        if next_is_h {
            if next.0 < curr.0 { sub.push((curr.0 - corner, curr.1)); } else { sub.push((curr.0 + corner, curr.1)); }
        } else if next.1 < curr.1 { sub.push((curr.0, curr.1 - corner)); } else { sub.push((curr.0, curr.1 + corner)); }
    }

    let start = (x + points[0].0 as f32, y + points[0].1 as f32);
    let mut d = format!("M {} {}", start.0, start.1);
    for chunk in sub.chunks(3) {
        if let [l, q1, q2] = chunk {
            let l_abs = (x + l.0, y + l.1);
            let q1_abs = (x + q1.0, y + q1.1);
            let q2_abs = (x + q2.0, y + q2.1);
            // Quadratic control q1 => cubic c1,c2
            let c1 = (
                l_abs.0 + (2.0_f32 / 3.0_f32) * (q1_abs.0 - l_abs.0),
                l_abs.1 + (2.0_f32 / 3.0_f32) * (q1_abs.1 - l_abs.1),
            );
            let c2 = (
                q2_abs.0 + (2.0_f32 / 3.0_f32) * (q1_abs.0 - q2_abs.0),
                q2_abs.1 + (2.0_f32 / 3.0_f32) * (q1_abs.1 - q2_abs.1),
            );
            d.push_str(&format!(" L {} {}", l_abs.0, l_abs.1));
            d.push_str(&format!(" C {} {}, {} {}, {} {}", c1.0, c1.1, c2.0, c2.1, q2_abs.0, q2_abs.1));
        }
    }
    let end = (x + points[points.len() - 1].0 as f32, y + points[points.len() - 1].1 as f32);
    d.push_str(&format!(" L {} {}", end.0, end.1));
    Some(d)
}

/// Helper struct for rendering glyphs with tiny-skia (implements OutlinePen)
struct TinySkiaPen<'a> {
    pixmap: &'a mut PixmapMut<'a>,
    x: f32,
    y: f32,
    paint: Paint<'static>,
    open_path: PathBuilder,
}

impl<'a> TinySkiaPen<'a> {
    fn new(pixmap: &'a mut PixmapMut<'a>) -> TinySkiaPen<'a> {
        TinySkiaPen {
            pixmap,
            x: 0.0,
            y: 0.0,
            paint: Paint::default(),
            open_path: PathBuilder::new(),
        }
    }

    fn set_origin(&mut self, x: f32, y: f32) {
        self.x = x;
        self.y = y;
    }

    fn set_color(&mut self, color: Color) {
        self.paint.set_color(color);
    }

    fn draw_glyph(
        &mut self,
        glyph: &OutlineGlyph<'_>,
        font_size: f32,
        normalized_coords: &[NormalizedCoord],
    ) {
        let settings = DrawSettings::unhinted(Size::new(font_size), LocationRef::new(normalized_coords));
        glyph.draw(settings, self).ok();
    }

    fn finish_path(&mut self) {
        let builder = std::mem::replace(&mut self.open_path, PathBuilder::new());
        if let Some(path) = builder.finish() {
            self.pixmap.fill_path(
                &path,
                &self.paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}

impl OutlinePen for TinySkiaPen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.open_path.move_to(self.x + x, self.y - y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.open_path.line_to(self.x + x, self.y - y);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.open_path
            .quad_to(self.x + cx0, self.y - cy0, self.x + x, self.y - y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.open_path.cubic_to(
            self.x + cx0,
            self.y - cy0,
            self.x + cx1,
            self.y - cy1,
            self.x + x,
            self.y - y,
        );
    }

    fn close(&mut self) {
        self.open_path.close();
    }
}

/// Context for text rendering operations
struct TextRenderContext<'a> {
    font_cx: &'a mut FontContext,
    layout_cx: &'a mut LayoutContext,
    custom_fonts: &'a std::collections::HashMap<String, Vec<u8>>,
}

/// Properties for rendering text
struct TextProperties<'a> {
    text: &'a str,
    x: f32,
    y: f32,
    font_size: f32,
    color: (u8, u8, u8, u8),
    font_family: &'static str,
    text_align: Option<&'a str>,
    container_width: f32,
}

/// Render text using Parley and tiny-skia
fn render_text<'a>(
    pixmap: &'a mut PixmapMut<'a>,
    props: &TextProperties<'a>,
    text_ctx: &mut TextRenderContext<'a>,
) {
    // Skip empty text
    if props.text.is_empty() {
        return;
    }
    
    // Check if we have a custom font for this family
    if let Some(font_data) = text_ctx.custom_fonts.get(props.font_family) {
        // Use skrifa to render directly with our custom font
        if let Ok(font_ref) = ReadFontsRef::new(font_data.as_slice()) {
            render_text_with_skrifa(
                pixmap, 
                props.text, 
                props.x, 
                props.y, 
                props.font_size, 
                props.color, 
                &font_ref, 
                props.text_align, 
                props.container_width
            );
            return;
        }
    }
    
    let display_scale = 1.0;
    
    // Create a layout builder with parley (fallback to system fonts)
    let mut builder = text_ctx.layout_cx.ranged_builder(text_ctx.font_cx, props.text, display_scale, false);
    
    // Set font properties with the specified font family
    builder.push_default(StyleProperty::FontStack(parley::style::FontStack::Source(props.font_family.into())));
    builder.push_default(StyleProperty::FontSize(props.font_size));
    
    // Build the layout
    let mut layout = builder.build(props.text);
    layout.break_all_lines(None);
    
    // Create pen for rendering
    let mut pen = TinySkiaPen::new(pixmap);
    let text_color = Color::from_rgba8(props.color.0, props.color.1, props.color.2, props.color.3);
    
    // Render each glyph run
    for line in layout.lines() {
        for item in line.items() {
            if let parley::PositionedLayoutItem::GlyphRun(glyph_run) = item {
                let mut run_x = glyph_run.offset();
                let run_y = glyph_run.baseline();
                
                let run = glyph_run.run();
                let font = run.font();
                let font_size = run.font_size();
                let normalized_coords = run
                    .normalized_coords()
                    .iter()
                    .map(|coord| NormalizedCoord::from_bits(*coord))
                    .collect::<Vec<_>>();
                
                // Get font outlines
                let font_collection_ref = font.data.as_ref();
                if let Ok(font_ref) = ReadFontsRef::from_index(font_collection_ref, font.index) {
                    let outlines = font_ref.outline_glyphs();
                    
                    // Render each glyph
                    for glyph in glyph_run.glyphs() {
                        let glyph_x = props.x + run_x + glyph.x;
                        let glyph_y = props.y + run_y - glyph.y;
                        run_x += glyph.advance;
                        
                        let glyph_id = GlyphId::from(glyph.id);
                        if let Some(glyph_outline) = outlines.get(glyph_id) {
                            pen.set_origin(glyph_x, glyph_y);
                            pen.set_color(text_color);
                            pen.draw_glyph(&glyph_outline, font_size, &normalized_coords);
                            pen.finish_path();
                        }
                    }
                }
            }
        }
    }
}

/// Render text directly using skrifa without parley
#[allow(clippy::too_many_arguments)]
fn render_text_with_skrifa<'a>(
    pixmap: &'a mut PixmapMut<'a>,
    text: &str,
    x: f32,
    y: f32,
    font_size: f32,
    color: (u8, u8, u8, u8),
    font_ref: &ReadFontsRef,
    text_align: Option<&str>,
    container_width: f32,
) {
    let mut pen = TinySkiaPen::new(pixmap);
    let text_color = Color::from_rgba8(color.0, color.1, color.2, color.3);
    
    let outlines = font_ref.outline_glyphs();
    let charmap = font_ref.charmap();
    let glyph_metrics = font_ref.glyph_metrics(Size::new(font_size), LocationRef::default());
    
    // Get font metrics for line height calculation
    let metrics = font_ref.metrics(Size::new(font_size), LocationRef::default());
    let line_height = (metrics.ascent - metrics.descent + metrics.leading) * 1.25; // 1.25 is typical line height multiplier
    
    let mut cursor_y = y;
    
    // Split text by newlines and render each line
    let lines: Vec<&str> = text.split('\n').collect();
    
    for (line_idx, line) in lines.iter().enumerate() {
        // Calculate line width for alignment
        let mut line_width = 0.0f32;
        for ch in line.chars() {
            if let Some(glyph_id) = charmap.map(ch) {
                if let Some(advance) = glyph_metrics.advance_width(glyph_id) {
                    line_width += advance;
                }
            }
        }
        
        // Calculate starting X position based on alignment
        let start_x = match text_align {
            Some("center") => x + (container_width - line_width) / 2.0,
            Some("right") => x + container_width - line_width,
            _ => x, // "left" or default
        };
        let mut cursor_x = start_x;
        
        // Render each character in the line
        for ch in line.chars() {
            if let Some(glyph_id) = charmap.map(ch) {
                if let Some(glyph_outline) = outlines.get(glyph_id) {
                    pen.set_origin(cursor_x, cursor_y);
                    pen.set_color(text_color);
                    pen.draw_glyph(&glyph_outline, font_size, &[]);
                    pen.finish_path();
                }
                // Advance cursor horizontally
                if let Some(advance) = glyph_metrics.advance_width(glyph_id) {
                    cursor_x += advance;
                }
            }
        }
        
        // Move to next line if not the last line
        if line_idx < lines.len() - 1 {
            cursor_y += line_height;
        }
    }
}

fn render_element<'a, 'b: 'a>(
    pixmap: &'a mut PixmapMut<'a>, 
    element: &'b Element,
    offset: (f32, f32),
    text_ctx: &mut TextRenderContext<'a>,
    transform: Transform,
) {
    if element.is_deleted {
        return;
    }
    
    // Extract scale factor from transform (sx for uniform scaling)
    let scale = transform.sx;
    
    // Transform coordinates relative to viewbox and apply scale
    let x = ((element.x - offset.0 as f64) * scale as f64) as f32;
    let y = ((element.y - offset.1 as f64) * scale as f64) as f32;
    let width = (element.width * scale as f64) as f32;
    let height = (element.height * scale as f64) as f32;

    // Parse colors
    let stroke_rgba = parse_color(&element.stroke_color);
    let fill_rgba = parse_color(&element.background_color);

    let has_stroke = !element.stroke_color.is_empty() 
        && element.stroke_color != "transparent" 
        && element.stroke_width > 0.0;
    
    let has_fill = !element.background_color.is_empty() 
        && element.background_color != "transparent";

    // Map Excalidraw fill styles to roughr FillStyle
    let fill_style = match element.fill_style.as_str() {
        "hachure" => FillStyle::Hachure,
        "cross-hatch" => FillStyle::CrossHatch,
        "solid" => FillStyle::Solid,
        _ => FillStyle::Hachure,
    };

    // Create rough options
    let mut options_builder = OptionsBuilder::default();
    
    if has_stroke {
        options_builder.stroke(
            Srgba::from_components((stroke_rgba.0, stroke_rgba.1, stroke_rgba.2, stroke_rgba.3))
                .into_format(),
        );
        options_builder.stroke_width((element.stroke_width * scale as f64) as f32);
    }
    
    if has_fill {
        options_builder.fill(
            Srgba::from_components((fill_rgba.0, fill_rgba.1, fill_rgba.2, fill_rgba.3))
                .into_format(),
        );
        options_builder.fill_style(fill_style);
    }

    // Set roughness
    options_builder.roughness(element.roughness as f32);
    options_builder.seed(element.seed as u64);
    // Stroke dash pattern per strokeStyle (use scaled stroke width)
    let scaled_stroke_width = (element.stroke_width * scale as f64) as f32;
    if let Some(dash) = exca_stroke_dash(&element.stroke_style, scaled_stroke_width) {
        // Prefer backend dash support if exposed by roughr
        #[allow(unused_must_use)]
        {
            let dash64: Vec<f64> = dash.into_iter().map(|v| v as f64).collect();
            options_builder.stroke_line_dash(dash64);
        }
    }
    
    // DPI for fill weight (scale with transform)
    const BASE_DPI: f32 = 96.0;
    let dpi = BASE_DPI * scale;
    options_builder.fill_weight(dpi * 0.01);

    let options = options_builder.build().unwrap();
    let generator = SkiaGenerator::new(options);

    // Helper: stroke-only generator for linear/arrow paths to avoid fill paths
    let mut stroke_only_builder = OptionsBuilder::default();
    if has_stroke {
        stroke_only_builder.stroke(
            Srgba::from_components((stroke_rgba.0, stroke_rgba.1, stroke_rgba.2, stroke_rgba.3))
                .into_format(),
        );
        stroke_only_builder.stroke_width((element.stroke_width * scale as f64) as f32);
    }
    stroke_only_builder.roughness(element.roughness as f32);
    stroke_only_builder.seed(element.seed as u64);
    stroke_only_builder.fill_weight(dpi * 0.01);
    if let Some(dash) = exca_stroke_dash(&element.stroke_style, scaled_stroke_width) {
        #[allow(unused_must_use)]
        {
            let dash64: Vec<f64> = dash.into_iter().map(|v| v as f64).collect();
            stroke_only_builder.stroke_line_dash(dash64);
        }
    }
    let stroke_only_options = stroke_only_builder.build().unwrap();
    let stroke_gen = SkiaGenerator::new(stroke_only_options);

    // Build a separate generator for arrowhead caps:
    // - solid for solid/dashed
    // - dotted with reduced gap for dotted
    let mut cap_builder = OptionsBuilder::default();
    if has_stroke {
        cap_builder.stroke(
            Srgba::from_components((stroke_rgba.0, stroke_rgba.1, stroke_rgba.2, stroke_rgba.3))
                .into_format(),
        );
        cap_builder.stroke_width((element.stroke_width * scale as f64) as f32);
    }
    cap_builder.roughness(element.roughness as f32);
    cap_builder.seed(element.seed as u64);
    cap_builder.fill_weight(dpi * 0.01);
    if element.stroke_style == "dotted" {
        #[allow(unused_must_use)]
        {
            let dash = exca_dotted_cap_dash(scaled_stroke_width);
            let dash64: Vec<f64> = dash.into_iter().map(|v| v as f64).collect();
            cap_builder.stroke_line_dash(dash64);
        }
    }
    let cap_gen = SkiaGenerator::new(cap_builder.build().unwrap());

    // Render based on element type
    match element.element_type.as_str() {
        "rectangle" => {
            // Check if rectangle has roundness
            if element.roundness.is_some() {
                let r = get_corner_radius(width.min(height), element);
                // Create rounded rectangle path using SVG-like path commands with x,y offsets
                // M (x+r) y L (x+w-r) y Q (x+w) y, (x+w) (y+r) L (x+w) (y+h-r)
                // Q (x+w) (y+h), (x+w-r) (y+h) L (x+r) (y+h) Q x (y+h), x (y+h-r)
                // L x (y+r) Q x y, (x+r) y
                let path_d = format!(
                    "M {} {} L {} {} Q {} {}, {} {} L {} {} Q {} {}, {} {} L {} {} Q {} {}, {} {} L {} {} Q {} {}, {} {}",
                    x + r, y,
                    x + width - r, y,
                    x + width, y, x + width, y + r,
                    x + width, y + height - r,
                    x + width, y + height, x + width - r, y + height,
                    x + r, y + height,
                    x, y + height, x, y + height - r,
                    x, y + r,
                    x, y, x + r, y
                );

                let rounded_rect = generator.path::<f32>(path_d);
                rounded_rect.draw(pixmap);
            } else {
                let rect = generator.rectangle::<f32>(x, y, width, height);
                rect.draw(pixmap);
            }
        }
        "ellipse" => {
            // rough.js ellipse expects center coordinates (cx, cy) and diameters (width, height)
            let cx = x + width / 2.0;
            let cy = y + height / 2.0;
            let ellipse = generator.ellipse::<f32>(cx, cy, width, height);
            ellipse.draw(pixmap);
        }
        "diamond" => {
            // Create diamond path using polygon
            let cx = x + width / 2.0;
            let cy = y + height / 2.0;
            let points = vec![
                Point2D::new(cx, y),           // top
                Point2D::new(x + width, cy),   // right
                Point2D::new(cx, y + height),  // bottom
                Point2D::new(x, cy),           // left
            ];
            let polygon = generator.polygon(&points);
            polygon.draw(pixmap);
        }
        "line" => {
            if let Some(ref points) = element.points {
                if points.len() >= 2 {
                    // Scale points coordinates
                    let scaled_points: Vec<(f64, f64)> = points.iter()
                        .map(|p| (p.0 * scale as f64, p.1 * scale as f64))
                        .collect();
                    if element.roundness.is_some() {
                        if let Some(path_d) = build_catmull_rom_cubic_path(&scaled_points, x, y) {
                            let path = stroke_gen.path::<f32>(path_d);
                            path.draw(pixmap);
                        }
                    } else {
                        // straight polyline via rough path M/L
                        let mut d = String::new();
                        d.push_str(&format!("M {} {}", x + scaled_points[0].0 as f32, y + scaled_points[0].1 as f32));
                        for p in scaled_points.iter().skip(1) {
                            d.push_str(&format!(" L {} {}", x + p.0 as f32, y + p.1 as f32));
                        }
                        let path = stroke_gen.path::<f32>(d);
                        path.draw(pixmap);
                    }
                }
            }
        }
        "arrow" => {
            if let Some(ref points) = element.points {
                if points.len() >= 2 {
                    // Scale points coordinates
                    let scaled_points: Vec<(f64, f64)> = points.iter()
                        .map(|p| (p.0 * scale as f64, p.1 * scale as f64))
                        .collect();
                    
                    // Draw curve or polyline depending on style
                    if element.elbowed.unwrap_or(false) && scaled_points.len() >= 3 {
                        if let Some(path_d) = build_elbow_arrow_cubic_path(&scaled_points, x, y, 16.0 * scale) {
                            let path = stroke_gen.path::<f32>(path_d);
                            path.draw(pixmap);
                        }
                    } else if element.roundness.is_some() && scaled_points.len() >= 2 {
                        if let Some(path_d) = build_catmull_rom_cubic_path(&scaled_points, x, y) {
                            let path = stroke_gen.path::<f32>(path_d);
                            path.draw(pixmap);
                        }
                    } else {
                        // straight polyline via rough path M/L
                        let mut d = String::new();
                        d.push_str(&format!("M {} {}", x + scaled_points[0].0 as f32, y + scaled_points[0].1 as f32));
                        for p in scaled_points.iter().skip(1) {
                            d.push_str(&format!(" L {} {}", x + p.0 as f32, y + p.1 as f32));
                        }
                        let path = stroke_gen.path::<f32>(d);
                        path.draw(pixmap);
                    }

                    // Draw start arrowhead if specified
                    if let Some(ref start_arrowhead) = element.start_arrowhead {
                        draw_arrowhead_ex(
                            pixmap,
                            &scaled_points,
                            x,
                            y,
                            stroke_rgba,
                            scaled_stroke_width,
                            start_arrowhead,
                            "start",
                            &cap_gen,
                        );
                    }
                    
                    // Draw end arrowhead if specified
                    if let Some(ref end_arrowhead) = element.end_arrowhead {
                        draw_arrowhead_ex(
                            pixmap,
                            &scaled_points,
                            x,
                            y,
                            stroke_rgba,
                            scaled_stroke_width,
                            end_arrowhead,
                            "end",
                            &cap_gen,
                        );
                    }
                }
            }
        }
        "text" => {
            // Render text element
            if let Some(ref text) = element.text {
                let font_size = (element.font_size.unwrap_or(20.0) * scale as f64) as f32;
                let font_family = get_font_family_for_id(element.font_family);
                // Create TextProperties with lifetimes tied to element
                let text_props = TextProperties {
                    text: text.as_str(),
                    x,
                    y: y + font_size,
                    font_size,
                    color: stroke_rgba,
                    font_family,
                    text_align: element.text_align.as_deref(),
                    container_width: width,
                };
                // Render text - the lifetime is satisfied because text_props only lives for this scope
                render_text(pixmap, &text_props, text_ctx);
            }
        }
        _ => {
            // Unsupported element type
            eprintln!("Unsupported element type: {}", element.element_type);
        }
    }
}

/// Load custom fonts from embedded bytes
fn load_custom_fonts() -> std::collections::HashMap<String, Vec<u8>> {
    let mut fonts = std::collections::HashMap::new();
    
    // Load fonts from embedded bytes
    fonts.insert("Liberation Sans".to_string(), LIBERATION_SANS_REGULAR.to_vec());
    fonts.insert("Cascadia Code".to_string(), CASCADIA_CODE.to_vec());
    fonts.insert("Excalifont".to_string(), EXCALIFONT_REGULAR.to_vec());
    
    eprintln!("Loaded {} custom fonts from embedded bytes", fonts.len());
    fonts
}

/// Get font family name based on Excalidraw font ID
/// Maps font IDs to family names that match the loaded fonts
fn get_font_family_for_id(font_id: Option<i32>) -> &'static str {
    match font_id {
        Some(1) => "Liberation Sans",
        Some(2) => "Cascadia Code",
        _ => "Excalifont", // Default or ID 0
    }
}

pub fn render_to_png(
    data: &ExcalidrawData,
    output_path: &std::path::Path,
    background: Option<(u8, u8, u8, u8)>,
    quality: u8,
    dpi: Option<u32>,
) -> Result<()> {
    let viewbox = calculate_viewbox(&data.elements);
    
    // Calculate scale factor from DPI (assume source is 96 DPI)
    const SOURCE_DPI: f32 = 96.0;
    let scale = dpi.map(|d| d as f32 / SOURCE_DPI).unwrap_or(1.0);
    
    let width = (viewbox.width * scale as f64).ceil() as u32;
    let height = (viewbox.height * scale as f64).ceil() as u32;
    
    let mut pixmap = Pixmap::new(width, height)
        .ok_or_else(|| anyhow::anyhow!("Failed to create pixmap"))?;

    // Fill background if provided (or default to white if None)
    if let Some((r, g, b, a)) = background.or(Some((255, 255, 255, 255))) {
        if a > 0 {
            let mut background_paint = Paint::default();
            background_paint.set_color_rgba8(r, g, b, a);
            pixmap.fill_rect(
                Rect::from_xywh(0.0, 0.0, width as f32, height as f32).unwrap(),
                &background_paint,
                Transform::identity(),
                None,
            );
        }
    }
    
    // Create font and layout contexts for text rendering
    let mut font_cx = FontContext::default();
    let mut layout_cx = LayoutContext::new();
    
    // Load custom fonts from the fonts directory
    let custom_fonts = load_custom_fonts();
    
    // Create transform matrix for scaling
    let transform = Transform::from_scale(scale, scale);
    
    // Render each element
    for element in &data.elements {
        // Create text rendering context for each element to avoid borrowing conflicts
        let mut text_ctx = TextRenderContext {
            font_cx: &mut font_cx,
            layout_cx: &mut layout_cx,
            custom_fonts: &custom_fonts,
        };
        
        render_element(
            &mut pixmap.as_mut(),
            element,
            (viewbox.min_x as f32, viewbox.min_y as f32),
            &mut text_ctx,
            transform,
        );
    }
    
    // Save to PNG with quality control
    save_png_with_quality(&pixmap, output_path, quality)?;
    
    Ok(())
}
