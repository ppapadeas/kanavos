//! Greek building regulation calculations (ΝΟΚ, ν. 4067/2012) and permit sheet presets.
//!
//! Layer L5. See `docs/architecture.md`.
//!
//! This crate turns model quantities into the figures checked in a Greek building permit: κάλυψη (coverage),
//! δόμηση (floor area counted towards the building coefficient) and their limits. Which spaces count, and how, is
//! defined in specs under `specs/rules-gr/`, each citing the article of the law it implements.

use kanavos_units::{Area, AreaDisplay, Locale};

/// Building terms that apply to a plot (όροι δόμησης).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildingTerms {
    /// Plot area (εμβαδόν οικοπέδου).
    pub plot_area: Area,
    /// Building coefficient (συντελεστής δόμησης, Σ.Δ.).
    pub building_coefficient: f64,
    /// Maximum coverage ratio (ποσοστό κάλυψης), 0.0–1.0.
    pub max_coverage_ratio: f64,
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
}

/// One live meter shown in the inspector: used vs allowed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meter {
    /// What the project uses.
    pub used: Area,
    /// What the terms allow.
    pub allowed: Area,
}

impl Meter {
    /// Remaining margin; negative when over the limit.
    pub fn margin(&self) -> Area {
        self.allowed - self.used
    }
    /// Whether the project is within the limit.
    pub fn within_limit(&self) -> bool {
        self.used.as_m2() <= self.allowed.as_m2()
    }
    /// Fraction of the limit used (0.0 when nothing is allowed and nothing is used).
    pub fn ratio(&self) -> f64 {
        if self.allowed.as_m2() > 0.0 {
            self.used.as_m2() / self.allowed.as_m2()
        } else if self.used.as_m2() > 0.0 {
            f64::INFINITY
        } else {
            0.0
        }
    }
    /// Short label such as `112,40 / 120,00 m²`.
    pub fn label(&self, locale: Locale) -> String {
        let used = AreaDisplay(self.used, locale).to_string();
        let used = used.trim_end_matches(" m²");
        format!("{used} / {}", AreaDisplay(self.allowed, locale))
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
        BuildingTerms { plot_area: Area::m2(500.0), building_coefficient: 0.8, max_coverage_ratio: 0.6 }
    }

    #[test]
    fn limits_follow_plot_area() {
        let t = terms();
        assert!((t.max_floor_area().as_m2() - 400.0).abs() < 1e-9);
        assert!((t.max_coverage().as_m2() - 300.0).abs() < 1e-9);
    }

    #[test]
    fn meters_report_margin_and_overrun() {
        let m = meters(&terms(), Area::m2(180.0), &[Area::m2(180.0), Area::m2(240.0)]);
        assert!(m.coverage.within_limit());
        assert!((m.coverage.margin().as_m2() - 120.0).abs() < 1e-9);
        assert!(!m.floor_area.within_limit());
        assert!((m.floor_area.margin().as_m2() + 20.0).abs() < 1e-9);
    }

    #[test]
    fn label_uses_locale() {
        let m = Meter { used: Area::m2(112.4), allowed: Area::m2(120.0) };
        assert_eq!(m.label(Locale::El), "112,40 / 120,00 m²");
    }
}
