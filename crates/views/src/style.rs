//! Pens, colours and phase styling shared by every derived view.
//!
//! Line weights follow the ISO 128-20 series in [`kanavos_drawing::weights`]. The heavier pens step down as the
//! scale gets smaller, so a 1:100 plan reads like a 1:50 plan reduced, not like a smudge (see
//! `specs/views/plan-drawing.md`).

use kanavos_drawing::{Fill, LinePattern, Rgba, Stroke, weights};
use kanavos_model::{Phase, SpaceCategory};

/// Line weights for one drawing scale, in paper millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pens {
    /// Outline of everything cut by the plan or section plane.
    pub cut: f64,
    /// Outline of a cut part inside another cut part (a column inside a wall).
    pub cut_minor: f64,
    /// Boundaries between materials inside a cut.
    pub boundary: f64,
    /// Visible edges below a plan's cut plane.
    pub below: f64,
    /// Visible edges beyond a section plane, near the plane.
    pub beyond: f64,
    /// Visible edges far beyond a section plane.
    pub far: f64,
    /// Dashed edges above a plan's cut plane.
    pub above: f64,
    /// Door and window symbols, furniture.
    pub symbol: f64,
    /// Dimension lines, extension lines, leaders.
    pub annotation: f64,
    /// Dimension ticks.
    pub tick: f64,
    /// Outer contour of an elevation or 3D view against the background.
    pub profile: f64,
    /// Ground line in sections and elevations.
    pub ground: f64,
}

impl Pens {
    /// The pen set for a scale denominator (50 means 1:50).
    pub fn for_scale(scale: f64) -> Pens {
        use weights::*;
        if scale <= 20.0 {
            Pens {
                cut: EXTRA_THICK,
                cut_minor: THICK,
                boundary: FINE,
                below: THIN,
                beyond: THIN,
                far: FINE,
                above: FINE,
                symbol: THIN,
                annotation: FINE,
                tick: THICK,
                profile: THICK,
                ground: HEAVY,
            }
        } else if scale <= 50.0 {
            Pens {
                cut: EXTRA_THICK,
                cut_minor: MEDIUM,
                boundary: EXTRA_FINE,
                below: THIN,
                beyond: THIN,
                far: FINE,
                above: FINE,
                symbol: THIN,
                annotation: FINE,
                tick: MEDIUM,
                profile: THICK,
                ground: EXTRA_THICK,
            }
        } else if scale <= 100.0 {
            Pens {
                cut: THICK,
                cut_minor: THIN,
                boundary: EXTRA_FINE,
                below: FINE,
                beyond: FINE,
                far: EXTRA_FINE,
                above: FINE,
                symbol: FINE,
                annotation: FINE,
                tick: MEDIUM,
                profile: MEDIUM,
                ground: THICK,
            }
        } else {
            Pens {
                cut: MEDIUM,
                cut_minor: FINE,
                boundary: EXTRA_FINE,
                below: EXTRA_FINE,
                beyond: EXTRA_FINE,
                far: EXTRA_FINE,
                above: EXTRA_FINE,
                symbol: EXTRA_FINE,
                annotation: EXTRA_FINE,
                tick: THIN,
                profile: THIN,
                ground: MEDIUM,
            }
        }
    }
}

/// Grey used for existing construction.
pub(crate) const EXISTING_LINE: Rgba = Rgba::rgb(110, 110, 110);
/// Fill of existing construction where it is cut.
pub(crate) const EXISTING_FILL: Rgba = Rgba::rgb(200, 200, 200);
/// Grey for underlays.
pub(crate) const UNDERLAY: Rgba = Rgba::rgb(150, 150, 150);

/// A stroke adjusted for an element's phase: existing work is grey, demolished work is dashed.
pub(crate) fn phase_stroke(mut s: Stroke, phase: Phase) -> Stroke {
    match phase {
        Phase::New => s,
        Phase::Existing => {
            s.color = EXISTING_LINE;
            s
        }
        Phase::Demolished => {
            if s.pattern == LinePattern::Solid {
                s.pattern = LinePattern::Hidden;
            }
            s
        }
    }
}

/// The fill of a cut area for a phase: new work keeps its material fill, existing work is solid grey,
/// demolished work is left white.
pub(crate) fn phase_fill(material_fill: Fill, phase: Phase) -> Fill {
    match phase {
        Phase::New => material_fill,
        Phase::Existing => Fill::Solid(EXISTING_FILL),
        Phase::Demolished => Fill::Solid(Rgba::WHITE),
    }
}

/// Light tint of a space by its use category.
pub(crate) fn space_tint(c: SpaceCategory) -> Rgba {
    match c {
        SpaceCategory::MainUse => Rgba::rgb(252, 247, 232),
        SpaceCategory::Auxiliary => Rgba::rgb(236, 241, 247),
        SpaceCategory::Stairwell => Rgba::rgb(242, 240, 236),
        SpaceCategory::LiftShaft => Rgba::rgb(232, 232, 232),
        SpaceCategory::SemiOpen => Rgba::rgb(236, 246, 232),
        SpaceCategory::Balcony => Rgba::rgb(232, 244, 246),
        SpaceCategory::Pilotis => Rgba::rgb(242, 242, 236),
        SpaceCategory::Parking => Rgba::rgb(238, 238, 238),
        SpaceCategory::Attic => Rgba::rgb(247, 241, 230),
        SpaceCategory::Technical => Rgba::rgb(244, 236, 236),
        SpaceCategory::Shaft => Rgba::rgb(232, 237, 243),
        SpaceCategory::Other => Rgba::rgb(245, 245, 245),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pens_get_lighter_at_smaller_scales() {
        let a = Pens::for_scale(50.0);
        let b = Pens::for_scale(100.0);
        let c = Pens::for_scale(200.0);
        assert!(a.cut > b.cut && b.cut > c.cut);
        assert!(a.cut >= weights::THICK && b.cut >= weights::THICK);
        assert!(a.boundary < a.below && a.below < a.cut);
    }

    #[test]
    fn demolished_lines_are_dashed_and_existing_grey() {
        let s = phase_stroke(Stroke::solid(0.5), Phase::Demolished);
        assert_eq!(s.pattern, LinePattern::Hidden);
        let e = phase_stroke(Stroke::solid(0.5), Phase::Existing);
        assert_eq!(e.color, EXISTING_LINE);
    }
}
