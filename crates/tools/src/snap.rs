//! Snapping and inference.

use kanavos_geom::{Angle, Point2, Segment2, Vec2};
use kanavos_i18n::t;
use kanavos_model::{Document, Id};
use kanavos_units::{Length, Locale};

/// Snap settings.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapSettings {
    /// Master switch (Alt disables temporarily).
    pub enabled: bool,
    /// Snap to element points (ends, midpoints, corners, centres).
    pub points: bool,
    /// Snap to the nearest point on edges and faces.
    pub edges: bool,
    /// Grid spacing; `None` disables grid snapping.
    pub grid: Option<Length>,
    /// Angle step for direction locking from the previous point, degrees (0 disables).
    pub angle_step: f64,
    /// Snap radius in screen pixels.
    pub radius_px: f64,
}

impl Default for SnapSettings {
    fn default() -> Self {
        SnapSettings {
            enabled: true,
            points: true,
            edges: true,
            grid: Some(Length::cm(5.0)),
            angle_step: 15.0,
            radius_px: 10.0,
        }
    }
}

/// Why the cursor snapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapHint {
    /// No snap.
    Free,
    /// An end point.
    Endpoint,
    /// A midpoint.
    Midpoint,
    /// A corner of an outline.
    Corner,
    /// A centre.
    Centre,
    /// The intersection of two edges.
    Intersection,
    /// The nearest point on an edge or face.
    OnEdge,
    /// Perpendicular from the previous point to an edge.
    Perpendicular,
    /// Along the extension of an edge.
    Extension,
    /// A locked angle from the previous point.
    Angle,
    /// A grid point.
    Grid,
}

impl SnapHint {
    /// Catalogue key of the on-screen label.
    pub fn key(self) -> &'static str {
        match self {
            SnapHint::Free => "tools.snap.free",
            SnapHint::Endpoint => "tools.snap.endpoint",
            SnapHint::Midpoint => "tools.snap.midpoint",
            SnapHint::Corner => "tools.snap.corner",
            SnapHint::Centre => "tools.snap.centre",
            SnapHint::Intersection => "tools.snap.intersection",
            SnapHint::OnEdge => "tools.snap.on_edge",
            SnapHint::Perpendicular => "tools.snap.perpendicular",
            SnapHint::Extension => "tools.snap.extension",
            SnapHint::Angle => "tools.snap.angle",
            SnapHint::Grid => "tools.snap.grid",
        }
    }
}

/// A snapped point.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapResult {
    /// The point to use.
    pub point: Point2,
    /// Why.
    pub hint: SnapHint,
    /// On-screen label, e.g. "μέσο" or "κάθετη".
    pub label: String,
    /// Guide lines to draw (inference lines).
    pub guides: Vec<Segment2>,
    /// The element snapped to, if any.
    pub element: Option<Id>,
}

/// Collects snap candidates for a storey; build once per document revision and storey, query per pointer move.
#[derive(Debug, Clone, Default)]
pub struct Snapper {
    points: Vec<(Point2, SnapHint, Id)>,
    segments: Vec<(Segment2, Id)>,
}

impl Snapper {
    /// Gathers snap geometry of the elements on a storey (and grid lines).
    pub fn new(doc: &Document, storey: Id) -> Snapper {
        let mut s = Snapper::default();
        for e in doc.elements() {
            if e.storey != Some(storey) && e.storey.is_some() {
                continue;
            }
            let g = kanavos_elements::snap_geometry(doc, e.id);
            for (p, k) in g.points {
                let hint = match k {
                    kanavos_elements::SnapKind::Endpoint => SnapHint::Endpoint,
                    kanavos_elements::SnapKind::Midpoint => SnapHint::Midpoint,
                    kanavos_elements::SnapKind::Centre => SnapHint::Centre,
                    kanavos_elements::SnapKind::Corner => SnapHint::Corner,
                    kanavos_elements::SnapKind::Insertion => SnapHint::Endpoint,
                };
                s.points.push((p, hint, e.id));
            }
            for seg in g.segments {
                s.segments.push((seg, e.id));
            }
        }
        s
    }

    /// Snaps a raw pointer position. `from` is the previous point of the operation, for angle locks and
    /// perpendiculars. `pixel` is the size of a screen pixel in metres.
    pub fn snap(
        &self,
        raw: Point2,
        from: Option<Point2>,
        settings: &SnapSettings,
        pixel: f64,
        locale: Locale,
    ) -> SnapResult {
        let free = SnapResult {
            point: raw,
            hint: SnapHint::Free,
            label: String::new(),
            guides: Vec::new(),
            element: None,
        };
        if !settings.enabled {
            return free;
        }
        let radius = settings.radius_px * pixel;
        let label = |h: SnapHint| t(locale, h.key()).into_owned();
        if settings.points {
            let best = self
                .points
                .iter()
                .map(|(p, h, id)| (p.distance(raw).as_m(), *p, *h, *id))
                .filter(|(d, ..)| *d <= radius)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, p, h, id)) = best {
                return SnapResult {
                    point: p,
                    hint: h,
                    label: label(h),
                    guides: Vec::new(),
                    element: Some(id),
                };
            }
        }
        if settings.edges {
            let best = self
                .segments
                .iter()
                .map(|(s, id)| (s.distance_to(raw).as_m(), *s, *id))
                .filter(|(d, ..)| *d <= radius)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, s, id)) = best {
                if let Some(f) = from {
                    let foot = s.closest_point(f);
                    if foot.distance(raw).as_m() <= radius {
                        return SnapResult {
                            point: foot,
                            hint: SnapHint::Perpendicular,
                            label: label(SnapHint::Perpendicular),
                            guides: vec![Segment2::new(f, foot)],
                            element: Some(id),
                        };
                    }
                }
                return SnapResult {
                    point: s.closest_point(raw),
                    hint: SnapHint::OnEdge,
                    label: label(SnapHint::OnEdge),
                    guides: Vec::new(),
                    element: Some(id),
                };
            }
        }
        if let Some(f) = from
            && settings.angle_step > 0.0
        {
            let v = raw - f;
            let len = v.length();
            if len > 1e-9 {
                let step = settings.angle_step.to_radians();
                let a = v.angle().as_rad();
                let snapped = (a / step).round() * step;
                if (a - snapped).abs() * len <= radius {
                    let dir = Vec2::from_angle(Angle::rad(snapped));
                    let mut length = len;
                    if let Some(g) = settings.grid {
                        let gs = g.as_m();
                        if gs > 0.0 {
                            length = (length / gs).round() * gs;
                        }
                    }
                    let p = f + dir * length;
                    return SnapResult {
                        point: p,
                        hint: SnapHint::Angle,
                        label: label(SnapHint::Angle),
                        guides: vec![Segment2::new(f, p)],
                        element: None,
                    };
                }
            }
        }
        if let Some(g) = settings.grid {
            let gs = g.as_m();
            if gs > 0.0 {
                let [x, y] = raw.xy();
                let p = Point2::new((x / gs).round() * gs, (y / gs).round() * gs);
                return SnapResult {
                    point: p,
                    hint: SnapHint::Grid,
                    label: String::new(),
                    guides: Vec::new(),
                    element: None,
                };
            }
        }
        free
    }
}
