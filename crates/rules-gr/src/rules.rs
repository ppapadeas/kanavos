//! The configurable parameters of the ΝΟΚ checks, with defaults from the law.
//!
//! Every limit the checks use lives here, so that a plot with special terms (or a later amendment) changes a
//! number, not code. Citations give the ΝΟΚ article (ν. 4067/2012 as amended, notably by ν. 4759/2020 and
//! ν. 5197/2025) and, in brackets, the article of the Spatial and Urban Planning Code ratified by ν. 5306/2026
//! (ΚΧΠ) that now carries the same text. `specs/rules-gr/` explains each rule and marks the uncertain points.

use kanavos_model::SpaceCategory;

/// One row of the height table: building coefficients up to `max_coefficient` allow `max_height` metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightStep {
    /// Upper bound of the building coefficient (Σ.Δ.) for this row, inclusive.
    pub max_coefficient: f64,
    /// Maximum building height for this row, metres.
    pub max_height: f64,
}

/// How a space category counts towards δόμηση on a storey, before area limits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Counting {
    /// Counts in full.
    Counted,
    /// Exempt in full.
    Exempt,
    /// This fraction counts, the rest is exempt (main use in a basement).
    Fraction(f64),
    /// Exempt up to [`NokRules::stair_exempt_per_level`] per stairwell and storey; the excess counts.
    StairLimit,
    /// Exempt up to [`NokRules::attic_exempt_fraction`] of the storey below; the excess counts.
    AtticLimit,
    /// Exempt when at least [`NokRules::shaft_min_area`]; smaller shafts count.
    ShaftMinimum,
    /// Open space (semi-open, balcony): exempt within the allowance of [`NokRules::open_space_allowance`].
    OpenSpaceAllowance,
}

/// What the height used for the setback formulas is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetbackHeight {
    /// The built height when the floor area allowance is used up, otherwise the maximum height of the terms
    /// (ΝΟΚ 3, definition of H [ΚΧΠ 198]).
    Law,
    /// Always the built height.
    Built,
}

/// Parameters of the ΝΟΚ checks.
#[derive(Debug, Clone, PartialEq)]
pub struct NokRules {
    /// Open balconies and semi-open spaces together are exempt from δόμηση up to this fraction of the allowed
    /// floor area (ΝΟΚ 11 §6 α' [ΚΧΠ 206 §6 α']).
    pub open_space_allowance: f64,
    /// Semi-open spaces alone may not exceed this fraction of the allowed floor area (same provision).
    pub semi_open_cap: f64,
    /// When open spaces exceed the allowance, their whole area counts (ΝΟΚ 11 §5 δ' [ΚΧΠ 206 §5 δ']); when
    /// false, only the excess counts.
    pub open_excess_counts_whole: bool,
    /// Stairs inside one dwelling are exempt up to this area per stair and storey, m² (ΝΟΚ 11 §6 ε'
    /// [ΚΧΠ 206 §6 ε']). Common stairs of several units allow 30 m² (40 m² at the entrance level, §6 δ').
    pub stair_exempt_per_level: f64,
    /// Light and ventilation shafts are exempt from this net area up, m² (ΝΟΚ 11 §6 η' [ΚΧΠ 206 §6 η']).
    pub shaft_min_area: f64,
    /// Fraction of main-use basement area that counts (ΝΟΚ 11 §6 ι' [ΚΧΠ 206 §6 ι']).
    pub basement_main_use_factor: f64,
    /// Attics are exempt up to this fraction of the storey below, walls included (ΝΟΚ 2 §81 and 11 §6 ιε'
    /// [ΚΧΠ 197 αρ. 81, 206 §6 ιδ']).
    pub attic_exempt_fraction: f64,
    /// Above-ground auxiliary spaces are exempt. The law exempts them only in basements (ΝΟΚ 11 §6 θ'–ι').
    pub auxiliary_above_ground_exempt: bool,
    /// Above-ground plant rooms are exempt. The law exempts them only in basements (ΝΟΚ 11 §6 θ').
    pub technical_above_ground_exempt: bool,
    /// Above-ground enclosed parking is exempt. The law exempts it only in parking buildings and basements
    /// (ΝΟΚ 11 §6 β', θα').
    pub parking_above_ground_exempt: bool,
    /// External thermal insulation, with plaster and cladding outside it, is left out of κάλυψη and δόμηση
    /// (ΝΟΚ 11 §6 ιθ' [ΚΧΠ 206 §6 ιη'], ΝΟΚ 12 §4 γ' [ΚΧΠ 207 §4 γ']).
    pub exclude_external_insulation: bool,
    /// Cladding beyond the insulation is exempt up to this thickness, metres (same provision).
    pub cladding_allowance: f64,
    /// Walls lower than this (metres) are fences, parapets or low partitions and are not building outline.
    /// This is a Kanavos modelling convention, not a provision of the law.
    pub low_wall_height: f64,
    /// A storey is a basement (υπόγειο) only if its ceiling is at most this many metres above the finished
    /// ground (ΝΟΚ 2 §87 [ΚΧΠ 197 αρ. 87]; 0.80 m outside town plans).
    pub basement_max_above_ground: f64,
    /// A pilotis must cover at least this fraction of the building footprint (ΝΟΚ 11 §6 ιζ' [ΚΧΠ 206 §6 ιστ']).
    pub pilotis_min_fraction: f64,
    /// The floor of a pilotis must be within this distance of the ground, metres (same provision).
    pub pilotis_max_level_difference: f64,
    /// Setback Δ = `delta_base + delta_factor × H` from the rear boundary (ΝΟΚ 3 [ΚΧΠ 198], ΝΟΚ 14 [ΚΧΠ 209]).
    pub delta_base: f64,
    /// See [`NokRules::delta_base`].
    pub delta_factor: f64,
    /// Setback δ = `small_delta_base + small_delta_factor × H` from side boundaries (same provisions).
    pub small_delta_base: f64,
    /// See [`NokRules::small_delta_base`].
    pub small_delta_factor: f64,
    /// Which height H the setback formulas use.
    pub setback_height: SetbackHeight,
    /// Maximum height by building coefficient, used when the terms give no height, lowest coefficient first
    /// (ΝΟΚ 15 §1 [ΚΧΠ 210 §1]).
    pub height_table: Vec<HeightStep>,
    /// Above the last row of the table the height is this factor × Σ.Δ. (same provision).
    pub height_factor_above_table: f64,
    /// Absolute cap of the height, metres (same provision).
    pub height_cap: f64,
    /// A pitched roof may rise this much above the maximum height, metres (ΝΟΚ 2 §91 [ΚΧΠ 197 αρ. 90]).
    pub roof_allowance: f64,
    /// Stair housings may rise this much above the roof slab, metres; 4.20 m where a lift stops there
    /// (ΝΟΚ 19 §2 [ΚΧΠ 214 §2]).
    pub housing_allowance: f64,
    /// See [`NokRules::housing_allowance`].
    pub housing_allowance_with_lift: f64,
    /// Where the coverage ratio is the general 60 %, coverage may reach this area, m², … (ΝΟΚ 12 §1
    /// [ΚΧΠ 207 §1]).
    pub coverage_min_area: f64,
    /// … but not more than this fraction of the plot (same provision).
    pub coverage_min_cap: f64,
    /// Volume coefficient Σ.Ο. = this factor × Σ.Δ. when the terms set none (ΝΟΚ 13 [ΚΧΠ 208]).
    pub volume_factor: f64,
    /// Volume factor where the maximum height is at most 8.50 m (same provision).
    pub volume_factor_low: f64,
}

impl Default for NokRules {
    fn default() -> Self {
        NokRules {
            open_space_allowance: 0.40,
            semi_open_cap: 0.20,
            open_excess_counts_whole: true,
            stair_exempt_per_level: 25.0,
            shaft_min_area: 1.50,
            basement_main_use_factor: 0.5,
            attic_exempt_fraction: 0.5,
            auxiliary_above_ground_exempt: false,
            technical_above_ground_exempt: false,
            parking_above_ground_exempt: false,
            exclude_external_insulation: true,
            cladding_allowance: 0.20,
            low_wall_height: 1.5,
            basement_max_above_ground: 1.20,
            pilotis_min_fraction: 0.5,
            pilotis_max_level_difference: 0.50,
            delta_base: 3.0,
            delta_factor: 0.10,
            small_delta_base: 2.5,
            small_delta_factor: 0.05,
            setback_height: SetbackHeight::Law,
            height_table: vec![
                HeightStep { max_coefficient: 0.4, max_height: 10.75 },
                HeightStep { max_coefficient: 0.8, max_height: 14.00 },
                HeightStep { max_coefficient: 1.2, max_height: 17.25 },
                HeightStep { max_coefficient: 1.6, max_height: 19.50 },
                HeightStep { max_coefficient: 2.0, max_height: 22.75 },
                HeightStep { max_coefficient: 2.6, max_height: 26.00 },
            ],
            height_factor_above_table: 10.0,
            height_cap: 32.0,
            roof_allowance: 2.0,
            housing_allowance: 3.40,
            housing_allowance_with_lift: 4.20,
            coverage_min_area: 120.0,
            coverage_min_cap: 0.70,
            volume_factor: 5.0,
            volume_factor_low: 5.5,
        }
    }
}

impl NokRules {
    /// How a space category counts towards δόμηση on an above-ground or below-ground storey.
    pub fn counting(&self, c: SpaceCategory, below_ground: bool) -> Counting {
        use Counting::*;
        let exempt_if = |b: bool| if b { Exempt } else { Counted };
        match c {
            SpaceCategory::MainUse | SpaceCategory::Other => {
                if below_ground {
                    Fraction(self.basement_main_use_factor)
                } else {
                    Counted
                }
            }
            SpaceCategory::Auxiliary => exempt_if(below_ground || self.auxiliary_above_ground_exempt),
            SpaceCategory::Parking => exempt_if(below_ground || self.parking_above_ground_exempt),
            SpaceCategory::Technical => exempt_if(below_ground || self.technical_above_ground_exempt),
            SpaceCategory::Stairwell | SpaceCategory::LiftShaft => StairLimit,
            SpaceCategory::Attic => AtticLimit,
            SpaceCategory::Shaft => ShaftMinimum,
            SpaceCategory::Pilotis => Exempt,
            SpaceCategory::Balcony | SpaceCategory::SemiOpen => {
                if below_ground {
                    Exempt
                } else {
                    OpenSpaceAllowance
                }
            }
        }
    }

    /// Maximum height for a building coefficient (ΝΟΚ 15 §1 [ΚΧΠ 210 §1]).
    pub fn max_height_for(&self, coefficient: f64) -> f64 {
        match self.height_table.iter().find(|s| coefficient <= s.max_coefficient + 1e-9) {
            Some(s) => s.max_height,
            None => (self.height_factor_above_table * coefficient).min(self.height_cap),
        }
    }

    /// Setback Δ (rear) for a height `h` metres.
    pub fn delta_for(&self, h: f64) -> f64 {
        self.delta_base + self.delta_factor * h.max(0.0)
    }

    /// Setback δ (sides) for a height `h` metres.
    pub fn small_delta_for(&self, h: f64) -> f64 {
        self.small_delta_base + self.small_delta_factor * h.max(0.0)
    }

    /// Allowed coverage for a plot: the terms' ratio, raised to [`NokRules::coverage_min_area`] (within
    /// [`NokRules::coverage_min_cap`]) where the ratio is the general 60 %.
    pub fn allowed_coverage(&self, plot_area: f64, ratio: f64) -> f64 {
        let base = plot_area * ratio;
        if ratio >= 0.60 - 1e-9 && base < self.coverage_min_area {
            base.max(self.coverage_min_area.min(plot_area * self.coverage_min_cap))
        } else {
            base
        }
    }

    /// Volume coefficient that applies when the terms set none: 5 × Σ.Δ., or 5.5 × Σ.Δ. where the maximum
    /// height is at most 8.50 m.
    pub fn default_volume_coefficient(&self, building_coefficient: f64, max_height: f64) -> f64 {
        let f = if max_height <= 8.5 + 1e-9 { self.volume_factor_low } else { self.volume_factor };
        f * building_coefficient
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_table_follows_the_coefficient() {
        let r = NokRules::default();
        assert!((r.max_height_for(0.8) - 14.0).abs() < 1e-9);
        assert!((r.max_height_for(0.81) - 17.25).abs() < 1e-9);
        assert!((r.max_height_for(3.0) - 30.0).abs() < 1e-9);
        assert!((r.max_height_for(4.0) - 32.0).abs() < 1e-9);
    }

    #[test]
    fn setbacks_grow_with_height() {
        let r = NokRules::default();
        assert!((r.delta_for(14.0) - 4.4).abs() < 1e-9);
        assert!((r.small_delta_for(14.0) - 3.2).abs() < 1e-9);
    }

    #[test]
    fn small_plots_get_120_square_metres_of_coverage() {
        let r = NokRules::default();
        // 60 % of 180 m² is 108 m²: raised to 120 m² (within 70 % = 126 m²).
        assert!((r.allowed_coverage(180.0, 0.6) - 120.0).abs() < 1e-9);
        // 60 % of 160 m² is 96 m²: raised only to 70 % = 112 m².
        assert!((r.allowed_coverage(160.0, 0.6) - 112.0).abs() < 1e-9);
        assert!((r.allowed_coverage(500.0, 0.6) - 300.0).abs() < 1e-9);
        assert!((r.allowed_coverage(180.0, 0.4) - 72.0).abs() < 1e-9);
    }

    #[test]
    fn basement_main_use_counts_half() {
        let r = NokRules::default();
        assert_eq!(r.counting(SpaceCategory::MainUse, true), Counting::Fraction(0.5));
        assert_eq!(r.counting(SpaceCategory::Auxiliary, true), Counting::Exempt);
        assert_eq!(r.counting(SpaceCategory::Auxiliary, false), Counting::Counted);
    }
}
