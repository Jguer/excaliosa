use crate::models::ExcalidrawElement;

use super::{PaintedPath, PathOp};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Bounds {
    fn empty() -> Self {
        Self {
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
        }
    }

    fn include(&mut self, x: f64, y: f64) -> Option<()> {
        (x.is_finite() && y.is_finite()).then(|| {
            self.min_x = self.min_x.min(x);
            self.min_y = self.min_y.min(y);
            self.max_x = self.max_x.max(x);
            self.max_y = self.max_y.max(y);
        })
    }

    fn finish(self) -> Option<Self> {
        [self.min_x, self.min_y, self.max_x, self.max_y]
            .into_iter()
            .all(f64::is_finite)
            .then_some(self)
    }

    fn center(self) -> (f64, f64) {
        (
            self.min_x / 2.0 + self.max_x / 2.0,
            self.min_y / 2.0 + self.max_y / 2.0,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LinearMeasurement {
    pub center: (f64, f64),
    pub rotated_bounds: Bounds,
}

pub(crate) type LinearMeasurements = std::collections::HashMap<usize, LinearMeasurement>;

/// Measure only the shaft outline. Raw points and then the stored box provide
/// finite fallbacks for empty or non-finite generated paths.
pub(crate) fn measure_linear(
    element: &ExcalidrawElement,
    outline: Option<&[PaintedPath]>,
) -> LinearMeasurement {
    let raw = raw_point_bounds(element);
    let stored = stored_box_bounds(element);
    let unrotated = outline
        .and_then(|paths| path_bounds(paths, None))
        .or(raw)
        .or(stored)
        .unwrap_or(Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.0,
            max_y: 0.0,
        });
    let center = unrotated.center();
    let rotation = Some((center, element.angle));
    let rotated_bounds = outline
        .and_then(|paths| path_bounds(paths, rotation))
        .or_else(|| {
            raw.map(|bounds| rotate_bounds(bounds, center, element.angle).unwrap_or(bounds))
        })
        .or_else(|| stored.and_then(|bounds| rotate_bounds(bounds, center, element.angle)))
        .or(stored)
        .unwrap_or(unrotated);

    LinearMeasurement {
        center,
        rotated_bounds,
    }
}

fn raw_point_bounds(element: &ExcalidrawElement) -> Option<Bounds> {
    let mut bounds = Bounds::empty();
    for &(x, y) in element.points.as_deref().unwrap_or(&[]) {
        // Skip a bad point while retaining any other finite raw geometry.
        let _ = bounds.include(element.x + x, element.y + y);
    }
    bounds.finish()
}

fn stored_box_bounds(element: &ExcalidrawElement) -> Option<Bounds> {
    let mut bounds = Bounds::empty();
    bounds.include(element.x, element.y)?;
    bounds.include(element.x + element.width, element.y + element.height)?;
    bounds.finish()
}

fn rotate_bounds(bounds: Bounds, center: (f64, f64), angle: f64) -> Option<Bounds> {
    let (sin, cos) = angle.sin_cos();
    let mut rotated = Bounds::empty();
    for (x, y) in [
        (bounds.min_x, bounds.min_y),
        (bounds.max_x, bounds.min_y),
        (bounds.max_x, bounds.max_y),
        (bounds.min_x, bounds.max_y),
    ] {
        let (dx, dy) = (x - center.0, y - center.1);
        rotated.include(
            center.0 + dx * cos - dy * sin,
            center.1 + dx * sin + dy * cos,
        )?;
    }
    rotated.finish()
}

/// Rotate coordinates before measuring so cubic roots and line endpoints are
/// accumulated in the same coordinates as the exported viewport.
fn path_bounds(paths: &[PaintedPath], rotation: Option<((f64, f64), f64)>) -> Option<Bounds> {
    let (sin, cos) = rotation.map_or((0.0, 1.0), |(_, angle)| angle.sin_cos());
    let transform = |x: f64, y: f64| {
        if let Some((center, _)) = rotation {
            let (dx, dy) = (x - center.0, y - center.1);
            (
                center.0 + dx * cos - dy * sin,
                center.1 + dx * sin + dy * cos,
            )
        } else {
            (x, y)
        }
    };

    let mut bounds = Bounds::empty();
    for path in paths {
        let mut current = None;
        let mut subpath_start = None;
        for op in &path.ops {
            match *op {
                PathOp::Move(x, y) => {
                    let point = transform(x, y);
                    bounds.include(point.0, point.1)?;
                    current = Some((x, y));
                    subpath_start = current;
                }
                PathOp::Line(x, y) => {
                    if let Some((px, py)) = current {
                        let point = transform(px, py);
                        bounds.include(point.0, point.1)?;
                    }
                    let point = transform(x, y);
                    bounds.include(point.0, point.1)?;
                    current = Some((x, y));
                }
                PathOp::Cubic(x1, y1, x2, y2, x, y) => {
                    let start = current.unwrap_or((x, y));
                    let points = [
                        transform(start.0, start.1),
                        transform(x1, y1),
                        transform(x2, y2),
                        transform(x, y),
                    ];
                    include_cubic(&mut bounds, points)?;
                    current = Some((x, y));
                }
                PathOp::Close => current = subpath_start,
            }
        }
    }
    bounds.finish()
}

fn include_cubic(bounds: &mut Bounds, points: [(f64, f64); 4]) -> Option<()> {
    if points
        .iter()
        .any(|point| !point.0.is_finite() || !point.1.is_finite())
    {
        return None;
    }
    let [p0, p1, p2, p3] = points;
    bounds.include(p0.0, p0.1)?;
    bounds.include(p3.0, p3.1)?;

    for axis in [0, 1] {
        let coordinate = |point: (f64, f64)| if axis == 0 { point.0 } else { point.1 };
        let (a, b, c, d) = (
            coordinate(p0),
            coordinate(p1),
            coordinate(p2),
            coordinate(p3),
        );
        let qa = 3.0 * (-a + 3.0 * b - 3.0 * c + d);
        let qb = 6.0 * (a - 2.0 * b + c);
        let qc = 3.0 * (b - a);
        for t in quadratic_roots(qa, qb, qc)? {
            if !t.is_finite() {
                return None;
            }
            if t > 0.0 && t < 1.0 {
                let u = 1.0 - t;
                let value =
                    u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d;
                let (x, y) = if axis == 0 {
                    (value, cubic_coordinate(p0.1, p1.1, p2.1, p3.1, t))
                } else {
                    (cubic_coordinate(p0.0, p1.0, p2.0, p3.0, t), value)
                };
                bounds.include(x, y)?;
            }
        }
    }
    Some(())
}

fn cubic_coordinate(a: f64, b: f64, c: f64, d: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
}

fn quadratic_roots(a: f64, b: f64, c: f64) -> Option<Vec<f64>> {
    if ![a, b, c].into_iter().all(f64::is_finite) {
        return None;
    }
    if a == 0.0 {
        return Some(if b == 0.0 { vec![] } else { vec![-c / b] });
    }
    let discriminant = b * b - 4.0 * a * c;
    if !discriminant.is_finite() {
        return None;
    }
    if discriminant < 0.0 {
        return Some(vec![]);
    }
    let root = discriminant.sqrt();
    let q = -0.5 * (b + root.copysign(b));
    Some(if q == 0.0 {
        vec![-b / (2.0 * a)]
    } else {
        vec![q / a, c / q]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Rgba;

    fn path(ops: Vec<PathOp>) -> PaintedPath {
        PaintedPath::stroke_only(ops, Rgba(0, 0, 0, 255), 1.0)
    }

    #[test]
    fn cubic_extrema_include_interior_derivative_roots() {
        let paths = [path(vec![
            PathOp::Move(0.0, 0.0),
            PathOp::Cubic(0.0, 120.0, 100.0, -120.0, 100.0, 0.0),
        ])];
        let bounds = path_bounds(&paths, None).unwrap();
        assert!(bounds.max_y > 30.0, "{bounds:?}");
        assert!(bounds.min_y < -30.0, "{bounds:?}");
        assert_eq!((bounds.min_x, bounds.max_x), (0.0, 100.0));
    }

    #[test]
    fn multiple_subpaths_and_degenerate_moves_are_measured() {
        let paths = [path(vec![
            PathOp::Move(10.0, 20.0),
            PathOp::Line(30.0, 40.0),
            PathOp::Move(-5.0, 50.0),
        ])];
        assert_eq!(
            path_bounds(&paths, None),
            Some(Bounds {
                min_x: -5.0,
                min_y: 20.0,
                max_x: 30.0,
                max_y: 50.0,
            })
        );
    }

    #[test]
    fn empty_and_non_finite_outlines_use_finite_fallbacks() {
        let empty = ExcalidrawElement {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
            angle: std::f64::consts::FRAC_PI_2,
            ..Default::default()
        };
        let measurement = measure_linear(&empty, Some(&[]));
        assert_eq!(measurement.center, (60.0, 45.0));
        let bounds = measurement.rotated_bounds;
        for (actual, expected) in [
            (bounds.min_x, 35.0),
            (bounds.min_y, -5.0),
            (bounds.max_x, 85.0),
            (bounds.max_y, 95.0),
        ] {
            assert!((actual - expected).abs() < 1e-9);
        }

        let element = ExcalidrawElement {
            x: 10.0,
            y: 20.0,
            points: Some(vec![(0.0, 0.0), (100.0, 50.0)]),
            ..Default::default()
        };
        let paths = [path(vec![PathOp::Move(f64::NAN, 0.0)])];
        let measurement = measure_linear(&element, Some(&paths));
        assert_eq!(measurement.center, (60.0, 45.0));
        assert_eq!(
            measurement.rotated_bounds,
            raw_point_bounds(&element).unwrap()
        );
    }
}
