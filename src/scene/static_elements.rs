//! Sticky-note paper and non-interactive iframe/embed placeholders.

use crate::color_utils::parse_color;
use crate::models::ExcalidrawElement;
use roughr::generator::Generator;

use super::build::{build_shape, drawable};
use super::{Drawable, PaintedPath, PathOp, Primitive, Rgba, TextAlign, TextPrimitive};

const STICKY_NOTE_MIN_SIZE: f64 = 75.0;
const STICKY_NOTE_PADDING: f64 = 16.0;
const STICKY_NOTE_SHADOW_OFFSET: f64 = 3.0;
const STICKY_NOTE_FOOTER_FONT_SIZE: f64 = 12.0;
const STICKY_NOTE_FOOTER_BASELINE_FROM_BOTTOM: f64 = 14.0;
const STICKY_NOTE_FOOTER_MIN_BODY_WIDTH_FOR_YEAR: f64 = 80.0;

pub(super) fn build_sticky_note(
    el: &ExcalidrawElement,
    label: Option<&ExcalidrawElement>,
) -> Vec<Drawable> {
    let shadow = sticky_note_path(el, true);
    let paper = sticky_note_path(el, false);
    let mut background = Rgba::from_tuple(parse_color(&el.background_color));
    if background.3 == 0 {
        background = Rgba(255, 223, 107, 255);
    }

    let paths = vec![
        PaintedPath {
            ops: shadow,
            fill: Some(Rgba(0, 0, 0, 41)),
            stroke: None,
            stroke_width: 0.0,
            dash: None,
            fill_even_odd: false,
        },
        PaintedPath {
            ops: paper.clone(),
            fill: Some(background),
            stroke: None,
            stroke_width: 0.0,
            dash: None,
            fill_even_odd: false,
        },
        // A full low-alpha edge stroke approximates upstream's clipped inner shadow.
        PaintedPath::stroke_only(paper, Rgba(0, 0, 0, 20), 1.0),
    ];
    let mut drawables = vec![drawable(el, Primitive::Paths(paths))];
    if let Some(footer) = sticky_note_footer(el, label) {
        drawables.push(drawable(el, Primitive::Text(footer)));
    }
    drawables
}

pub(super) fn build_embed(generator: &Generator, el: &ExcalidrawElement) -> Vec<Drawable> {
    let mut drawables = vec![drawable(el, Primitive::Paths(build_shape(generator, el)))];
    if let Some(label) = embed_label(el) {
        drawables.push(drawable(el, Primitive::Text(label)));
    }
    drawables
}

fn embed_label(el: &ExcalidrawElement) -> Option<TextPrimitive> {
    let content = if el.element_type == "iframe" {
        "IFrame element".to_string()
    } else {
        match el.link.as_deref() {
            None | Some("") => "Empty Web-Embed".to_string(),
            Some(link) => link
                .chars()
                .map(|c| {
                    if c.is_control() || matches!(c, '\u{fffe}' | '\u{ffff}') {
                        ' '
                    } else {
                        c
                    }
                })
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        }
    };
    let available_width = el.width - 20.0;
    let available_height = el.height - 20.0;
    if content.is_empty() || available_width < 10.0 || available_height < 10.0 {
        return None;
    }

    let length = content.chars().count();
    let mut font_size = 16.0_f64.min(available_height);
    let fitted_size = available_width / length as f64;
    if fitted_size >= 10.0 {
        font_size = font_size.min(fitted_size);
    }
    // One em per character is a conservative bound for either text backend.
    let max_chars = (available_width / font_size).floor() as usize;
    if max_chars == 0 {
        return None;
    }
    let content = if length > max_chars {
        let mut shortened: String = content.chars().take(max_chars - 1).collect();
        shortened.push('…');
        shortened
    } else {
        content
    };
    let stroke_color = el.stroke_color.trim();
    let color = if stroke_color.eq_ignore_ascii_case("transparent") || stroke_color.is_empty() {
        Rgba(0, 0, 0, 255)
    } else {
        Rgba::from_tuple(parse_color(&el.stroke_color))
    };

    Some(TextPrimitive {
        content,
        x: el.x,
        y: el.y,
        baseline_offset: el.height / 2.0 + font_size * 0.35,
        font_size,
        font_family: "Liberation Sans",
        color,
        align: TextAlign::Center,
        line_height: font_size * 1.15,
        container_width: el.width,
    })
}

#[derive(Clone, Copy)]
enum NoteCommand {
    Move((f64, f64)),
    Line((f64, f64)),
    Quadratic((f64, f64), (f64, f64)),
}

fn sticky_note_path(el: &ExcalidrawElement, shadow: bool) -> Vec<PathOp> {
    let (offset, seed_offset) = if shadow {
        (STICKY_NOTE_SHADOW_OFFSET, 1)
    } else {
        (0.0, 0)
    };
    let points = sticky_note_points(el, offset, seed_offset);
    let radius = if el.roundness.is_some() {
        (el.width.min(el.height) * 0.04).clamp(0.0, 16.0)
    } else {
        0.0
    };
    let lifted_corner = if el.roughness == 2.0 {
        (SeededRandom::new(el.seed).next() * points.len() as f64).floor() as usize
    } else {
        usize::MAX
    };

    if radius == 0.0 && lifted_corner == usize::MAX {
        return vec![
            PathOp::Move(points[0].0, points[0].1),
            PathOp::Line(points[1].0, points[1].1),
            PathOp::Line(points[2].0, points[2].1),
            PathOp::Line(points[3].0, points[3].1),
            PathOp::Close,
        ];
    }

    let mut corners = Vec::with_capacity(points.len());
    for (index, &point) in points.iter().enumerate() {
        let previous = points[(index + points.len() - 1) % points.len()];
        let next = points[(index + 1) % points.len()];
        let corner_radius = radius
            .min(distance(point, previous) / 2.0)
            .min(distance(point, next) / 2.0);
        let mut commands = Vec::new();

        if index == lifted_corner {
            let size = el.width.min(el.height);
            let reach = (size * 0.18).min(40.0);
            let lift = (size * 0.02).min(5.0) * if shadow { 0.5 } else { 1.0 };
            let tip = (
                point.0
                    + if index == 1 || index == 2 {
                        -lift
                    } else {
                        lift
                    },
                point.1 + if index >= 2 { -lift } else { lift },
            );
            let start = point_at_distance(point, previous, reach);
            let end = point_at_distance(point, next, reach);
            commands.push(NoteCommand::Line(start));
            commands.push(NoteCommand::Quadratic(
                point_at_distance(point, previous, reach / 2.0),
                point_at_distance(tip, start, corner_radius),
            ));
            commands.push(NoteCommand::Quadratic(
                tip,
                point_at_distance(tip, end, corner_radius),
            ));
            commands.push(NoteCommand::Quadratic(
                point_at_distance(point, next, reach / 2.0),
                end,
            ));
        } else {
            commands.push(NoteCommand::Line(point_at_distance(
                point,
                previous,
                corner_radius,
            )));
            commands.push(NoteCommand::Quadratic(
                point,
                point_at_distance(point, next, corner_radius),
            ));
        }
        corners.push(commands);
    }

    let mut commands = vec![match corners[0].last() {
        Some(NoteCommand::Line(point) | NoteCommand::Quadratic(_, point)) => {
            NoteCommand::Move(*point)
        }
        _ => unreachable!(),
    }];
    for index in 1..=corners.len() {
        commands.extend(corners[index % corners.len()].iter().copied());
    }

    let mut ops = Vec::new();
    let mut current = (0.0, 0.0);
    for command in commands {
        match command {
            NoteCommand::Move(point) => {
                ops.push(PathOp::Move(point.0, point.1));
                current = point;
            }
            NoteCommand::Line(point) => {
                ops.push(PathOp::Line(point.0, point.1));
                current = point;
            }
            NoteCommand::Quadratic(control, point) => {
                let control1 = (
                    current.0 + (control.0 - current.0) * 2.0 / 3.0,
                    current.1 + (control.1 - current.1) * 2.0 / 3.0,
                );
                let control2 = (
                    point.0 + (control.0 - point.0) * 2.0 / 3.0,
                    point.1 + (control.1 - point.1) * 2.0 / 3.0,
                );
                ops.push(PathOp::Cubic(
                    control1.0, control1.1, control2.0, control2.1, point.0, point.1,
                ));
                current = point;
            }
        }
    }
    ops.push(PathOp::Close);
    ops
}

fn sticky_note_points(el: &ExcalidrawElement, offset: f64, seed_offset: i32) -> [(f64, f64); 4] {
    let roughness = el.roughness.round().clamp(0.0, 2.0) as usize;
    let render_roughness: f64 = [0.0, 1.5, 8.0][roughness];
    let amount = render_roughness.min((el.width.min(el.height) * 0.012).max(0.0));
    let mut random = SeededRandom::new(el.seed.wrapping_add(seed_offset));
    let mut point = |x: f64, y: f64| {
        if amount == 0.0 {
            (el.x + offset + x, el.y + offset + y)
        } else {
            (
                el.x + offset + x + (random.next() * 2.0 - 1.0) * amount,
                el.y + offset + y + (random.next() * 2.0 - 1.0) * amount,
            )
        }
    };
    [
        point(0.0, 0.0),
        point(el.width, 0.0),
        point(el.width, el.height),
        point(0.0, el.height),
    ]
}

fn point_at_distance(from: (f64, f64), to: (f64, f64), amount: f64) -> (f64, f64) {
    let length = distance(from, to);
    if length == 0.0 {
        return from;
    }
    let ratio = (amount / length).min(1.0);
    (
        from.0 + (to.0 - from.0) * ratio,
        from.1 + (to.1 - from.1) * ratio,
    )
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

struct SeededRandom(u32);

impl SeededRandom {
    fn new(seed: i32) -> Self {
        Self(seed as u32)
    }

    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b79f5);
        let mut next = self.0;
        next = (next ^ (next >> 15)).wrapping_mul(next | 1);
        next ^= next.wrapping_add((next ^ (next >> 7)).wrapping_mul(next | 61));
        f64::from(next ^ (next >> 14)) / 4_294_967_296.0
    }
}

fn sticky_note_footer(
    el: &ExcalidrawElement,
    label: Option<&ExcalidrawElement>,
) -> Option<TextPrimitive> {
    if el.width < STICKY_NOTE_MIN_SIZE || el.height < STICKY_NOTE_MIN_SIZE {
        return None;
    }
    let footer_top =
        el.y + el.height - STICKY_NOTE_FOOTER_BASELINE_FROM_BOTTOM - STICKY_NOTE_FOOTER_FONT_SIZE;
    if label.is_some_and(|label| {
        label.x < el.x + el.width
            && label.x + label.width > el.x
            && label.y < el.y + el.height
            && label.y + label.height > footer_top
    }) {
        return None;
    }
    let short = el.width - STICKY_NOTE_PADDING * 2.0 < STICKY_NOTE_FOOTER_MIN_BODY_WIDTH_FOR_YEAR;
    let content = utc_date_label(el.created?, short)?;
    Some(TextPrimitive {
        content,
        x: el.x,
        y: el.y,
        baseline_offset: el.height - STICKY_NOTE_FOOTER_BASELINE_FROM_BOTTOM,
        font_size: STICKY_NOTE_FOOTER_FONT_SIZE,
        font_family: "Liberation Sans",
        color: Rgba::from_tuple(parse_color(&el.stroke_color)),
        align: TextAlign::Right,
        line_height: STICKY_NOTE_FOOTER_FONT_SIZE * 1.15,
        container_width: el.width - STICKY_NOTE_PADDING,
    })
}

fn utc_date_label(created: f64, short: bool) -> Option<String> {
    // ECMAScript Date's finite range, evaluated in UTC rather than local time.
    if !created.is_finite() || created.abs() > 8_640_000_000_000_000.0 {
        return None;
    }
    let days = (created.trunc() / 86_400_000.0).floor() as i64;
    let (year, month, day) = civil_from_days(days);
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut label = format!("{day} {}", MONTHS[(month - 1) as usize]);
    if !short {
        label.push_str(&format!(" {year}"));
    }
    Some(label)
}

// Howard Hinnant's civil-from-days algorithm; day zero is 1970-01-01.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_dates_are_deterministic_utc_and_bounded_by_note_size() {
        assert_eq!(utc_date_label(0.0, false).as_deref(), Some("1 Jan 1970"));
        assert_eq!(utc_date_label(-1.0, false).as_deref(), Some("31 Dec 1969"));
        assert_eq!(
            utc_date_label(1_704_067_200_000.0, true).as_deref(),
            Some("1 Jan")
        );
        assert_eq!(
            utc_date_label(1_709_164_800_000.0, false).as_deref(),
            Some("29 Feb 2024")
        );
        assert_eq!(utc_date_label(f64::NAN, false), None);
        assert_eq!(utc_date_label(8_640_000_000_000_001.0, false), None);

        let note = ExcalidrawElement {
            element_type: "stickynote".into(),
            width: 100.0,
            height: 75.0,
            created: Some(0.0),
            ..Default::default()
        };
        assert_eq!(sticky_note_footer(&note, None).unwrap().content, "1 Jan");
        let mut wide = note.clone();
        wide.width = 200.0;
        assert_eq!(
            sticky_note_footer(&wide, None).unwrap().content,
            "1 Jan 1970"
        );
        wide.height = 74.0;
        assert!(sticky_note_footer(&wide, None).is_none());
    }

    #[test]
    fn lifted_note_geometry_is_seeded_and_shadow_is_offset() {
        let note = ExcalidrawElement {
            element_type: "stickynote".into(),
            x: 10.0,
            y: 20.0,
            width: 200.0,
            height: 150.0,
            roughness: 2.0,
            seed: 42,
            ..Default::default()
        };
        let paper = sticky_note_path(&note, false);
        assert_eq!(paper, sticky_note_path(&note, false));
        assert!(paper.iter().any(|op| matches!(op, PathOp::Cubic(..))));
        let shadow = sticky_note_path(&note, true);
        assert_ne!(paper, shadow);
    }

    #[test]
    fn transparent_embed_labels_stay_opaque_black() {
        for stroke_color in ["transparent", " TransParent "] {
            let embed = ExcalidrawElement {
                element_type: "iframe".into(),
                width: 200.0,
                height: 100.0,
                stroke_color: stroke_color.into(),
                ..Default::default()
            };
            assert_eq!(embed_label(&embed).unwrap().color, Rgba(0, 0, 0, 255));
        }
    }

    #[test]
    fn long_embed_labels_stay_single_line_and_bounded() {
        let empty = ExcalidrawElement {
            element_type: "embeddable".into(),
            width: 200.0,
            height: 100.0,
            ..Default::default()
        };
        assert_eq!(embed_label(&empty).unwrap().content, "Empty Web-Embed");

        let embed = ExcalidrawElement {
            element_type: "embeddable".into(),
            width: 100.0,
            height: 60.0,
            link: Some("<script>alert('a very long\0 malicious label')</script>".into()),
            ..Default::default()
        };
        let label = embed_label(&embed).unwrap();
        assert_eq!(label.content.chars().count(), 5);
        assert!(label.content.ends_with('…'));
        assert!(!label.content.chars().any(char::is_control));
        assert!(
            embed_label(&ExcalidrawElement {
                element_type: "embeddable".into(),
                width: 20.0,
                height: 20.0,
                link: Some("https://example.com".into()),
                ..Default::default()
            })
            .is_none()
        );
    }
}
