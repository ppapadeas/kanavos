//! Greek building regulation calculations (ΝΟΚ, ν. 4067/2012) and permit sheet presets.
//!
//! Layer L5. See `docs/architecture.md`.
//!
//! This crate turns the model into the figures checked in a Greek building permit: κάλυψη (coverage), δόμηση
//! (floor area counted towards the building coefficient), όγκος (volume), height, number of floors and setbacks,
//! with the drawings and tables that present them (διάγραμμα κάλυψης, site plan, πίνακας εμβαδών). Which spaces
//! count, and how, is defined in specs under `specs/rules-gr/`, each citing the provision it implements; every
//! number is a parameter in [`NokRules`].

mod annotate;
mod decompose;
mod diagram;
mod footprint;
mod georef;
mod ops;
mod report;
mod rules;
mod setback;
mod tables;

pub use annotate::{north_arrow, vertex_name};
pub use decompose::{AreaPiece, decompose};
pub use diagram::{coverage_diagram, site_plan};
pub use footprint::{StoreyOutline, building_footprint, building_footprint_with, storey_outline};
pub use georef::{grid_north, to_grid};
pub use report::{
    Finding, Floors, NokReport, OpenSpaces, Param, Severity, StoreyAreas, nok_report, nok_report_with,
    percent,
};
pub use rules::{Counting, HeightStep, NokRules, SetbackHeight};
pub use setback::{EdgeCheck, EdgeKind, SetbackReport, setbacks};
pub use tables::{
    area_table, area_table_of, building_coordinates_table, building_coordinates_table_of, building_vertices,
    coordinates_table, coverage_table, coverage_table_of, setback_table, setback_table_of, terms_table,
    terms_table_of,
};

use kanavos_units::{Area, AreaDisplay, Length, LengthDisplay, Locale, Volume, VolumeDisplay};

/// Building terms that apply to a plot (όροι δόμησης).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildingTerms {
    /// Plot area (εμβαδόν οικοπέδου).
    pub plot_area: Area,
    /// Building coefficient (συντελεστής δόμησης, Σ.Δ.).
    pub building_coefficient: f64,
    /// Maximum coverage ratio (ποσοστό κάλυψης), 0.0–1.0.
    pub max_coverage_ratio: f64,
    /// Volume coefficient (συντελεστής όγκου, Σ.Ο.), when the terms set one.
    pub volume_coefficient: Option<f64>,
}

impl BuildingTerms {
    /// Maximum floor area allowed (μέγιστη επιτρεπόμενη δόμηση).
    pub fn max_floor_area(&self) -> Area {
        Area::m2(self.plot_area.as_m2() * self.building_coefficient)
    }
    /// Maximum covered area allowed (μέγιστη επιτρεπόμενη κάλυψη).
    pub fn max_coverage(&self) -> Area {
        Area::m2(self.plot_area.as_m2() * self.max_coverage_ratio)
    }
    /// Maximum volume allowed (μέγιστος επιτρεπόμενος όγκος), when a volume coefficient applies.
    pub fn max_volume(&self) -> Option<Volume> {
        self.volume_coefficient.map(|k| Volume::m3(self.plot_area.as_m2() * k))
    }
}

/// A quantity a [`Meter`] can measure.
pub trait Quantity: Copy + std::ops::Sub<Output = Self> {
    /// The value in base units (m, m², m³).
    fn value(self) -> f64;
    /// The quantity with its unit for a locale, e.g. `120,00 m²`.
    fn display(self, locale: Locale) -> String;
}

impl Quantity for Area {
    fn value(self) -> f64 {
        self.as_m2()
    }
    fn display(self, locale: Locale) -> String {
        AreaDisplay(self, locale).to_string()
    }
}

impl Quantity for Length {
    fn value(self) -> f64 {
        self.as_m()
    }
    fn display(self, locale: Locale) -> String {
        LengthDisplay(self, locale).to_string()
    }
}

impl Quantity for Volume {
    fn value(self) -> f64 {
        self.as_m3()
    }
    fn display(self, locale: Locale) -> String {
        VolumeDisplay(self, locale).to_string()
    }
}

/// One live meter shown in the inspector: used vs allowed. Areas by default; heights and volumes use
/// `Meter<Length>` and `Meter<Volume>`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meter<Q = Area> {
    /// What the project uses.
    pub used: Q,
    /// What the terms allow.
    pub allowed: Q,
}

impl<Q: Quantity> Meter<Q> {
    /// Remaining margin; negative when over the limit.
    pub fn margin(&self) -> Q {
        self.allowed - self.used
    }
    /// Whether the project is within the limit (a 0.005 unit rounding slack keeps displayed equal values in).
    pub fn within_limit(&self) -> bool {
        self.used.value() <= self.allowed.value() + 0.005
    }
    /// Fraction of the limit used (0.0 when nothing is allowed and nothing is used).
    pub fn ratio(&self) -> f64 {
        let (u, a) = (self.used.value(), self.allowed.value());
        if a > 0.0 {
            u / a
        } else if u > 0.0 {
            f64::INFINITY
        } else {
            0.0
        }
    }
    /// Short label such as `112,40 / 120,00 m²`.
    pub fn label(&self, locale: Locale) -> String {
        let used = self.used.display(locale);
        let number = used.split(' ').next().unwrap_or_default();
        format!("{number} / {}", self.allowed.display(locale))
    }
}

/// Coverage and floor-area meters for a project.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meters {
    /// Coverage (κάλυψη).
    pub coverage: Meter,
    /// Floor area counted towards the building coefficient (δόμηση).
    pub floor_area: Meter,
}

/// Computes the meters from the counted areas.
///
/// `covered_area` is the building footprint counted as coverage, and `counted_floor_areas` the per-storey areas
/// counted towards δόμηση, both already filtered by the ΝΟΚ rules for exempt spaces.
pub fn meters(terms: &BuildingTerms, covered_area: Area, counted_floor_areas: &[Area]) -> Meters {
    Meters {
        coverage: Meter { used: covered_area, allowed: terms.max_coverage() },
        floor_area: Meter {
            used: counted_floor_areas.iter().copied().sum(),
            allowed: terms.max_floor_area(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms() -> BuildingTerms {
        BuildingTerms {
            plot_area: Area::m2(500.0),
            building_coefficient: 0.8,
            max_coverage_ratio: 0.6,
            volume_coefficient: Some(2.4),
        }
    }

    #[test]
    fn limits_follow_plot_area() {
        let t = terms();
        assert!((t.max_floor_area().as_m2() - 400.0).abs() < 1e-9);
        assert!((t.max_coverage().as_m2() - 300.0).abs() < 1e-9);
        assert!(t.max_volume().is_some_and(|v| (v.as_m3() - 1200.0).abs() < 1e-9));
    }

    #[test]
    fn meters_report_margin_and_overrun() {
        let m = meters(&terms(), Area::m2(180.0), &[Area::m2(180.0), Area::m2(240.0)]);
        assert!(m.coverage.within_limit());
        assert!((m.coverage.margin().as_m2() - 120.0).abs() < 1e-9);
        assert!(!m.floor_area.within_limit());
        assert!((m.floor_area.margin().as_m2() + 20.0).abs() < 1e-9);
        assert!((m.floor_area.ratio() - 1.05).abs() < 1e-9);
    }

    #[test]
    fn label_uses_locale() {
        let m = Meter { used: Area::m2(112.4), allowed: Area::m2(120.0) };
        assert_eq!(m.label(Locale::El), "112,40 / 120,00 m²");
        let h = Meter { used: Length::m(7.4), allowed: Length::m(11.0) };
        assert_eq!(h.label(Locale::En), "7.40 / 11.00 m");
    }
}
