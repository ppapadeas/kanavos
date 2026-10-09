//! Mapping between drawing coordinates and screen points, and the navigation maths built on it.
//!
//! Drawing coordinates have Y up (metres for model views, millimetres for sheets); screen points have Y down.
//! Everything here is pure arithmetic in `f64`, converted to `f32` only at the very end, so that zooming far into
//! a large project stays precise.

use kanavos_drawing::{DrawingUnits, Pt, Rect, pt};

/// Zoom limits in screen points per drawing unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomLimits {
    /// Smallest zoom.
    pub min: f64,
    /// Largest zoom.
    pub max: f64,
}

impl ZoomLimits {
    /// Limits for a drawing unit: from a few kilometres across the screen to sub-millimetre detail for model
    /// drawings, and from a whole A0 sheet as a stamp to 0.01 mm per point for sheets.
    pub fn for_units(units: DrawingUnits) -> ZoomLimits {
        match units {
            DrawingUnits::Metres => ZoomLimits { min: 0.2, max: 200_000.0 },
            DrawingUnits::Millimetres => ZoomLimits { min: 0.02, max: 200.0 },
        }
    }

    /// The zoom clamped to the limits; non-finite values become the minimum.
    pub fn clamp(&self, zoom: f64) -> f64 {
        if zoom.is_finite() { zoom.clamp(self.min, self.max) } else { self.min }
    }
}

/// The view of a canvas: which drawing point sits at the centre of the screen rectangle, and how many screen
/// points one drawing unit spans.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewTransform {
    /// Drawing point at the centre of the canvas.
    pub centre: Pt,
    /// Screen points per drawing unit.
    pub zoom: f64,
    /// Centre of the canvas on screen (points).
    pub screen_centre: [f64; 2],
}

impl ViewTransform {
    /// A view centred on `centre` at `zoom`, for a canvas whose centre is at `screen_centre`.
    pub fn new(centre: Pt, zoom: f64, screen_centre: [f64; 2]) -> ViewTransform {
        ViewTransform { centre, zoom, screen_centre }
    }

    /// Screen position of a drawing point.
    pub fn to_screen(self, p: Pt) -> [f64; 2] {
        let [cx, cy] = self.screen_centre;
        [cx + (p.x - self.centre.x) * self.zoom, cy - (p.y - self.centre.y) * self.zoom]
    }

    /// Screen position of a drawing point as an egui position.
    pub fn pos(&self, p: Pt) -> egui::Pos2 {
        let [x, y] = self.to_screen(p);
        egui::pos2(x as f32, y as f32)
    }

    /// Drawing point under a screen position.
    pub fn to_drawing(self, s: [f64; 2]) -> Pt {
        let [cx, cy] = self.screen_centre;
        let z = if self.zoom > 0.0 { self.zoom } else { 1.0 };
        pt(self.centre.x + (s[0] - cx) / z, self.centre.y - (s[1] - cy) / z)
    }

    /// Drawing point under an egui position.
    pub fn drawing_at(&self, p: egui::Pos2) -> Pt {
        self.to_drawing([f64::from(p.x), f64::from(p.y)])
    }

    /// Size of one screen point in drawing units.
    pub fn unit_per_point(&self) -> f64 {
        if self.zoom > 0.0 { 1.0 / self.zoom } else { 1.0 }
    }

    /// Moves the view so that the drawing follows a screen drag of `(dx, dy)` points.
    pub fn pan(&mut self, dx: f64, dy: f64) {
        let k = self.unit_per_point();
        self.centre.x -= dx * k;
        self.centre.y += dy * k;
    }

    /// Multiplies the zoom by `factor` keeping the drawing point under `anchor` (screen) fixed. The result is
    /// clamped to `limits`.
    pub fn zoom_about(&mut self, anchor: [f64; 2], factor: f64, limits: &ZoomLimits) {
        if !(factor.is_finite() && factor > 0.0) {
            return;
        }
        let before = self.to_drawing(anchor);
        self.zoom = limits.clamp(self.zoom * factor);
        let after = self.to_drawing(anchor);
        self.centre.x += before.x - after.x;
        self.centre.y += before.y - after.y;
    }

    /// The drawing rectangle visible in a screen rectangle of `size` points centred on the canvas centre.
    pub fn visible(&self, size: [f64; 2]) -> Rect {
        let k = self.unit_per_point();
        let (hw, hh) = (size[0] * k / 2.0, size[1] * k / 2.0);
        Rect {
            min: pt(self.centre.x - hw, self.centre.y - hh),
            max: pt(self.centre.x + hw, self.centre.y + hh),
        }
    }
}

/// Centre and zoom that fit `bounds` in a canvas of `size` points, leaving `margin` points free on every side.
/// Degenerate bounds (a point or a line) get a minimum extent of `min_extent` drawing units.
pub fn fit(bounds: &Rect, size: [f64; 2], margin: f64, min_extent: f64, limits: &ZoomLimits) -> (Pt, f64) {
    let w = bounds.width().max(min_extent);
    let h = bounds.height().max(min_extent);
    let avail_w = (size[0] - 2.0 * margin).max(1.0);
    let avail_h = (size[1] - 2.0 * margin).max(1.0);
    let zoom = limits.clamp((avail_w / w).min(avail_h / h));
    (bounds.centre(), zoom)
}

/// Whether two drawing rectangles overlap (touching counts).
pub fn intersects(a: &Rect, b: &Rect) -> bool {
    a.min.x <= b.max.x && b.min.x <= a.max.x && a.min.y <= b.max.y && b.min.y <= a.max.y
}

/// Whether `inner` lies completely inside `outer`.
pub fn contains_rect(outer: &Rect, inner: &Rect) -> bool {
    outer.min.x <= inner.min.x
        && outer.min.y <= inner.min.y
        && inner.max.x <= outer.max.x
        && inner.max.y <= outer.max.y
}

/// A smooth animation of the view towards a target, in log-zoom space so that zooming feels uniform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewAnimation {
    /// Target centre.
    pub centre: Pt,
    /// Target zoom.
    pub zoom: f64,
}

impl ViewAnimation {
    /// Advances `view` towards the target by the fraction `t` (0..1) of the remaining distance. Returns `true`
    /// when the target is reached (within a hundredth of a point on screen).
    pub fn step(&self, view: &mut ViewTransform, t: f64) -> bool {
        let t = t.clamp(0.0, 1.0);
        let lz0 = view.zoom.max(1e-12).ln();
        let lz1 = self.zoom.max(1e-12).ln();
        let lz = lz0 + (lz1 - lz0) * t;
        view.zoom = lz.exp();
        view.centre.x += (self.centre.x - view.centre.x) * t;
        view.centre.y += (self.centre.y - view.centre.y) * t;
        let screen_err = (self.centre.x - view.centre.x).hypot(self.centre.y - view.centre.y) * view.zoom;
        let zoom_err = (lz1 - lz).abs();
        if screen_err < 0.01 && zoom_err < 1e-4 {
            view.centre = self.centre;
            view.zoom = self.zoom;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn screen_and_drawing_round_trip_with_y_flipped() {
        let v = ViewTransform::new(pt(10.0, 5.0), 40.0, [400.0, 300.0]);
        let s = v.to_screen(pt(11.0, 6.0));
        assert!(close(s[0], 440.0) && close(s[1], 260.0), "{s:?}");
        let back = v.to_drawing(s);
        assert!(close(back.x, 11.0) && close(back.y, 6.0));
    }

    #[test]
    fn zoom_about_keeps_the_anchor_fixed() {
        let mut v = ViewTransform::new(pt(0.0, 0.0), 50.0, [500.0, 400.0]);
        let anchor = [620.0, 150.0];
        let before = v.to_drawing(anchor);
        v.zoom_about(anchor, 2.5, &ZoomLimits::for_units(DrawingUnits::Metres));
        let after = v.to_drawing(anchor);
        assert!(close(before.x, after.x) && close(before.y, after.y));
        assert!(close(v.zoom, 125.0));
    }

    #[test]
    fn zoom_is_clamped() {
        let mut v = ViewTransform::new(pt(0.0, 0.0), 50.0, [0.0, 0.0]);
        let limits = ZoomLimits { min: 1.0, max: 100.0 };
        v.zoom_about([0.0, 0.0], 1000.0, &limits);
        assert!(close(v.zoom, 100.0));
        v.zoom_about([0.0, 0.0], 1e-9, &limits);
        assert!(close(v.zoom, 1.0));
        v.zoom_about([0.0, 0.0], f64::NAN, &limits);
        assert!(close(v.zoom, 1.0));
    }

    #[test]
    fn pan_follows_the_pointer() {
        let mut v = ViewTransform::new(pt(0.0, 0.0), 10.0, [100.0, 100.0]);
        let grabbed = v.to_drawing([120.0, 80.0]);
        v.pan(30.0, -20.0);
        let s = v.to_screen(grabbed);
        assert!(close(s[0], 150.0) && close(s[1], 60.0), "{s:?}");
    }

    #[test]
    fn fit_shows_the_whole_box_with_margins() {
        let b = Rect { min: pt(0.0, 0.0), max: pt(20.0, 10.0) };
        let limits = ZoomLimits::for_units(DrawingUnits::Metres);
        let (c, z) = fit(&b, [1000.0, 600.0], 50.0, 1.0, &limits);
        assert!(close(c.x, 10.0) && close(c.y, 5.0));
        assert!(close(z, 45.0));
        let v = ViewTransform::new(c, z, [500.0, 300.0]);
        let vis = v.visible([1000.0, 600.0]);
        assert!(contains_rect(&vis, &b));
    }

    #[test]
    fn animation_converges() {
        let mut v = ViewTransform::new(pt(0.0, 0.0), 10.0, [0.0, 0.0]);
        let a = ViewAnimation { centre: pt(5.0, -3.0), zoom: 80.0 };
        let mut done = false;
        for _ in 0..200 {
            if a.step(&mut v, 0.3) {
                done = true;
                break;
            }
        }
        assert!(done);
        assert!(close(v.zoom, 80.0) && close(v.centre.x, 5.0));
    }

    #[test]
    fn rectangles_overlap_and_contain() {
        let a = Rect { min: pt(0.0, 0.0), max: pt(2.0, 2.0) };
        let b = Rect { min: pt(1.0, 1.0), max: pt(3.0, 3.0) };
        let c = Rect { min: pt(5.0, 5.0), max: pt(6.0, 6.0) };
        assert!(intersects(&a, &b) && !intersects(&a, &c));
        assert!(contains_rect(&Rect { min: pt(-1.0, -1.0), max: pt(4.0, 4.0) }, &b));
        assert!(!contains_rect(&a, &b));
    }
}
