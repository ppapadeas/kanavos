//! Design checks that the interface shows next to an element: warnings and notes, never hard errors.
//!
//! Limits for stairs and guards come from the Greek building regulation (Κτιριοδομικός Κανονισμός, Υ.Α.
//! ΥΠΕΝ/ΔΑΟΚΑ/66006/2360, ΦΕΚ Β΄ 3985/22.06.2023): article 11 (stairs) and article 13 (guards).
//!
//! Article 11 §4 tabulates three stair types. Riser 0.13–0.18 m (type I), 0.13–0.20 m (type II), 0.13–0.18 m
//! (type III, curved flights); going at least 0.28 m (I) or 0.25 m (II, III); twice the riser plus the going
//! 0.60–0.66 m; clear headroom 2.20 m; at most 3.60 m between landings; landings at least 1.20 m (I, III) or
//! 0.90 m (II) along the flight. §4.1–§4.5 say which types each use of the building may have, and §2 sets the
//! clear width. [`stair_limits`] maps each [`StairUse`] onto these rules:
//!
//! | Use | Types | Riser max | Going min | Width min | Landing min |
//! | --- | --- | --- | --- | --- | --- |
//! | Internal stair of a dwelling (default) | I–III (§4.1) | 0.20 (type I: 0.18, noted) | 0.25 (type I: 0.28, noted) | 0.60 (§2) | 0.90 |
//! | Residential, shared stair | I–III (§4.1) | 0.20 (0.18 noted) | 0.25 (0.28 noted) | 0.90 up to three storeys, else 1.20 (§2) | 0.90 |
//! | Other uses (Β, Ζ, Η, Θ, Ι, Κ, Λ) | I or II (§4.2) | 0.20 (0.18 noted) | 0.25 (0.28 noted) | 0.90 (type II); 1.20 noted (§2) | 0.90 |
//! | Assembly, education (Γ, Δ) | I (§4.4) | 0.18 | 0.28 | 1.20 | 1.20 |
//! | Health and social care (Ε) | I (§4.3) | 0.18 | 0.28 | 1.20 | 1.20 |
//! | Auxiliary rooms only (§4.5) | relaxed | 0.23 | 0.23 | 0.60 | — |
//!
//! All uses except the auxiliary one also check the minimum riser (0.13 m), the step rule, the rise between
//! landings and the headroom. The exceptions of §4.3 and §4.4 (type II or III stairs that patients, children or
//! more than 20 people never use) are a design decision the checks cannot see; such stairs can be given the use
//! "other uses".

use kanavos_geom::{Polygon2, PolygonWithHoles, intersection, total_area};
use kanavos_i18n::{Locale, tf};
use kanavos_model::{Document, ElementKind, Id, RailingInfill, StairShape, StairUse};
use kanavos_units::format_decimal;

use crate::stair::{first_flight_risers, stair_layout};
use crate::{GeometryError, element};

/// How serious a check result is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CheckLevel {
    /// Worth knowing; within the rules.
    Note,
    /// Outside a regulation limit or a comfort rule.
    Warning,
}

/// One check result. The message comes from the i18n catalogue with `{value}` and `{limit}` filled in.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    /// Stable code, e.g. `"stair.riser_high"`.
    pub code: &'static str,
    /// Severity.
    pub level: CheckLevel,
    /// Measured value (metres, or a count).
    pub value: f64,
    /// The limit it was compared with.
    pub limit: f64,
    /// Where the limit comes from, e.g. `"ΚΚ 2023 άρθρο 11 §4"`.
    pub source: &'static str,
}

impl Check {
    /// Catalogue key of the message.
    pub fn key(&self) -> String {
        format!("elements.check.{}", self.code)
    }

    /// The message in a language, with numbers formatted for it.
    pub fn message(&self, locale: Locale) -> String {
        let decimals = if self.code.ends_with("count") { 0 } else { 2 };
        tf(
            locale,
            &self.key(),
            &[
                ("value", &format_decimal(self.value, decimals, locale)),
                ("limit", &format_decimal(self.limit, decimals, locale)),
                ("source", self.source),
            ],
        )
    }
}

const KK_STAIRS: &str = "ΚΚ 2023 άρθρο 11 §4";
const KK_STAIRS_WIDTH: &str = "ΚΚ 2023 άρθρο 11 §2";
const KK_STAIRS_AUXILIARY: &str = "ΚΚ 2023 άρθρο 11 §4.5";
const KK_GUARDS: &str = "ΚΚ 2023 άρθρο 13";

/// Riser limits (metres): minimum, maximum for type II, maximum for type I.
pub const RISER_MIN: f64 = 0.13;
/// Maximum riser height for a type II stair.
pub const RISER_MAX: f64 = 0.20;
/// Maximum riser height for a type I stair.
pub const RISER_MAX_TYPE_I: f64 = 0.18;
/// Minimum tread going for a type II stair.
pub const GOING_MIN: f64 = 0.25;
/// Minimum tread going for a type I stair.
pub const GOING_MIN_TYPE_I: f64 = 0.28;
/// Range of twice the riser plus the going.
pub const STEP_RULE: (f64, f64) = (0.60, 0.66);
/// Largest rise between two landings.
pub const MAX_FLIGHT_RISE: f64 = 3.60;
/// Smallest clear width of the internal stair of a single dwelling.
pub const WIDTH_MIN_DWELLING: f64 = 0.60;
/// Smallest clear width of a type II stair (dwellings of up to three storeys).
pub const WIDTH_MIN_TYPE_II: f64 = 0.90;
/// Smallest clear width of a type I stair, and of at least one stair in every building (§2).
pub const WIDTH_MIN_TYPE_I: f64 = 1.20;
/// Smallest landing length along the flight axis for a type II stair.
pub const LANDING_MIN_TYPE_II: f64 = 0.90;
/// Smallest landing length along the flight axis for a type I stair.
pub const LANDING_MIN_TYPE_I: f64 = 1.20;
/// Smallest clear headroom above the treads and landings.
pub const HEADROOM_MIN: f64 = 2.20;
/// Relaxed limits of §4.5 for stairs to auxiliary rooms: maximum riser, minimum going.
pub const AUXILIARY_RISER_MAX: f64 = 0.23;
/// Minimum going of a stair to auxiliary rooms (§4.5).
pub const AUXILIARY_GOING_MIN: f64 = 0.23;
/// Smallest guard height.
pub const GUARD_MIN: f64 = 1.00;
/// Largest height of a solid guard.
pub const SOLID_GUARD_MAX: f64 = 1.20;
/// Largest horizontal gap in a guard (for gaps taller than 0.25 m).
pub const GUARD_GAP_MAX: f64 = 0.15;
/// Overlap (m²) below which a slab only touches a tread rather than covering it.
const HEADROOM_OVERLAP: f64 = 1e-4;
/// Most storeys above ground a residential building may have for its shared stair to be 0.90 m wide (§2).
pub const RESIDENTIAL_NARROW_STOREYS: usize = 3;

/// The limits of article 11 that apply to a stair, from its use (see the module documentation for the mapping).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StairLimits {
    /// Smallest riser, if limited.
    pub riser_min: Option<f64>,
    /// Largest riser.
    pub riser_max: f64,
    /// Smallest going.
    pub going_min: f64,
    /// Allowed range of twice the riser plus the going, if limited.
    pub step_rule: Option<(f64, f64)>,
    /// Smallest clear width.
    pub width_min: f64,
    /// Width below which a note says the building needs another, wider stair (§2), if any.
    pub width_note: Option<f64>,
    /// Smallest landing length along the flight, if limited.
    pub landing_min: Option<f64>,
    /// Largest rise between landings, if limited.
    pub flight_rise_max: Option<f64>,
    /// Smallest clear headroom, if limited.
    pub headroom_min: Option<f64>,
    /// Whether limits of type I that are stricter than the ones checked are noted (uses that may choose type I
    /// or type II).
    pub note_type_i: bool,
    /// Source of the riser, going and landing limits.
    pub source: &'static str,
    /// Source of the width limit.
    pub width_source: &'static str,
}

/// The limits for a stair of a use in a building with `storeys_above_ground` storeys (pilotis included,
/// basements not; only the shared stair of a residential building depends on it).
pub fn stair_limits(usage: StairUse, storeys_above_ground: usize) -> StairLimits {
    let type_ii = StairLimits {
        riser_min: Some(RISER_MIN),
        riser_max: RISER_MAX,
        going_min: GOING_MIN,
        step_rule: Some(STEP_RULE),
        width_min: WIDTH_MIN_TYPE_II,
        width_note: None,
        landing_min: Some(LANDING_MIN_TYPE_II),
        flight_rise_max: Some(MAX_FLIGHT_RISE),
        headroom_min: Some(HEADROOM_MIN),
        note_type_i: true,
        source: KK_STAIRS,
        width_source: KK_STAIRS_WIDTH,
    };
    let type_i = StairLimits {
        riser_max: RISER_MAX_TYPE_I,
        going_min: GOING_MIN_TYPE_I,
        width_min: WIDTH_MIN_TYPE_I,
        landing_min: Some(LANDING_MIN_TYPE_I),
        note_type_i: false,
        width_source: KK_STAIRS,
        ..type_ii
    };
    match usage {
        StairUse::DwellingInternal => StairLimits { width_min: WIDTH_MIN_DWELLING, ..type_ii },
        StairUse::Residential => StairLimits {
            width_min: if storeys_above_ground <= RESIDENTIAL_NARROW_STOREYS {
                WIDTH_MIN_TYPE_II
            } else {
                WIDTH_MIN_TYPE_I
            },
            ..type_ii
        },
        StairUse::OtherUses => StairLimits { width_note: Some(WIDTH_MIN_TYPE_I), ..type_ii },
        StairUse::AssemblyEducation | StairUse::HealthCare => type_i,
        StairUse::Auxiliary => StairLimits {
            riser_min: None,
            riser_max: AUXILIARY_RISER_MAX,
            going_min: AUXILIARY_GOING_MIN,
            step_rule: None,
            width_min: WIDTH_MIN_DWELLING,
            width_note: None,
            landing_min: None,
            flight_rise_max: None,
            headroom_min: None,
            note_type_i: false,
            source: KK_STAIRS_AUXILIARY,
            width_source: KK_STAIRS_AUXILIARY,
        },
    }
}

/// Checks a stair against the comfort rule and the regulation limits for its use ([`stair_limits`]). An empty
/// list means no remarks.
///
/// Headroom: for every tread and landing, the clear height up to the underside of any slab whose outline (holes
/// excluded) covers part of it by more than 1 cm²; roofs and beams are not considered.
///
/// Failure modes: the element is not a stair or its layout cannot be computed.
pub fn stair_checks(doc: &Document, id: Id) -> Result<Vec<Check>, GeometryError> {
    let e = element(doc, id)?;
    let ElementKind::Stair(s) = &e.kind else {
        return Err(GeometryError::WrongKind { id, expected: "stair" });
    };
    let layout = stair_layout(doc, id)?;
    let above_ground = doc.storeys().iter().filter(|st| !st.below_ground).count();
    let lim = stair_limits(s.usage, above_ground);
    let r = layout.riser_height.as_m();
    let g = s.going.as_m();
    let w = s.width.as_m();
    let mut out = Vec::new();
    let mut push = |code, level, value, limit, source| out.push(Check { code, level, value, limit, source });
    if r > lim.riser_max + 1e-9 {
        push("stair.riser_high", CheckLevel::Warning, r, lim.riser_max, lim.source);
    } else if lim.note_type_i && r > RISER_MAX_TYPE_I + 1e-9 {
        push("stair.riser_type_ii", CheckLevel::Note, r, RISER_MAX_TYPE_I, lim.source);
    }
    if let Some(min) = lim.riser_min
        && r < min - 1e-9
    {
        push("stair.riser_low", CheckLevel::Warning, r, min, lim.source);
    }
    if g < lim.going_min - 1e-9 {
        push("stair.going_short", CheckLevel::Warning, g, lim.going_min, lim.source);
    } else if lim.note_type_i && g < GOING_MIN_TYPE_I - 1e-9 {
        push("stair.going_type_ii", CheckLevel::Note, g, GOING_MIN_TYPE_I, lim.source);
    }
    if let Some((low, high)) = lim.step_rule {
        let step = 2.0 * r + g;
        if step < low - 1e-9 {
            push("stair.step_rule_low", CheckLevel::Warning, step, low, lim.source);
        } else if step > high + 1e-9 {
            push("stair.step_rule_high", CheckLevel::Warning, step, high, lim.source);
        }
    }
    if w < lim.width_min - 1e-9 {
        push("stair.width_narrow", CheckLevel::Warning, w, lim.width_min, lim.width_source);
    } else if let Some(note) = lim.width_note
        && w < note - 1e-9
    {
        push("stair.width_one_main", CheckLevel::Note, w, note, KK_STAIRS_WIDTH);
    }
    // Rise between landings.
    let flights: Vec<u32> = match s.shape {
        StairShape::Straight => vec![s.risers],
        _ => {
            let first = first_flight_risers(s);
            vec![first, s.risers.saturating_sub(first)]
        }
    };
    if let Some(max) = lim.flight_rise_max {
        let longest = flights.iter().copied().max().unwrap_or(0);
        let flight_rise = r * f64::from(longest);
        if flight_rise > max + 1e-9 {
            push("stair.flight_rise", CheckLevel::Warning, flight_rise, max, lim.source);
        }
    }
    if let Some(min) = lim.landing_min
        && !layout.landings.is_empty()
        && w < min - 1e-9
    {
        push("stair.landing_short", CheckLevel::Warning, w, min, lim.source);
    }
    // The rise must match the risers drawn (the model divides it evenly; flag absurd counts).
    if s.risers > 0 && f64::from(s.risers) * RISER_MIN > s.rise.as_m() + 1e-9 && r < RISER_MIN {
        push(
            "stair.riser_count",
            CheckLevel::Note,
            f64::from(s.risers),
            (s.rise.as_m() / RISER_MIN).floor(),
            lim.source,
        );
    }
    // The stair should arrive at the floor of the storey above.
    if let Some(above) = e.storey.and_then(|st| doc.storey_above(st)) {
        let needed = above.elevation.as_m() - layout.base;
        if (s.rise.as_m() - needed).abs() > 0.01 {
            push("stair.rise_mismatch", CheckLevel::Note, s.rise.as_m(), needed, "");
        }
    }
    if let Some(min) = lim.headroom_min
        && let Some(clear) = headroom(doc, &layout)
        && clear < min - 1e-9
    {
        push("stair.headroom", CheckLevel::Warning, clear, min, lim.source);
    }
    Ok(out)
}

/// The smallest clear height above the treads and landings of a stair, up to the underside of slabs that cover
/// them; `None` when no slab covers any of them.
fn headroom(doc: &Document, layout: &crate::StairLayout) -> Option<f64> {
    let mut steps: Vec<(&Polygon2, f64)> = Vec::new();
    for f in &layout.flights {
        for (i, tread) in f.treads.iter().enumerate() {
            let z = layout.tread_elevations.get(f.first_riser + i).copied().unwrap_or(layout.base);
            steps.push((tread, z));
        }
    }
    steps.extend(layout.landings.iter().zip(layout.landing_elevations.iter().copied()));
    let mut lowest: Option<f64> = None;
    for e in doc.elements() {
        let ElementKind::Slab(slab) = &e.kind else { continue };
        let floor = e.storey.and_then(|st| doc.storey(st)).map(|st| st.elevation.as_m()).unwrap_or(0.0);
        let thickness = doc.structure_thickness(&slab.structure).map(|t| t.as_m()).unwrap_or(0.0);
        let top = floor + slab.top_offset.as_m();
        let underside = top - thickness;
        let Some(sb) = slab.outline.bbox() else { continue };
        for (step, z) in &steps {
            // Slabs at or below the step do not limit it; one the step runs into leaves no headroom.
            if top <= z + 1e-6 {
                continue;
            }
            let clear = (underside - z).max(0.0);
            if lowest.is_some_and(|l| clear >= l) {
                continue;
            }
            let Some(b) = step.bbox() else { continue };
            if !b.intersects(&sb) {
                continue;
            }
            let tread: PolygonWithHoles = step.to_ccw().into();
            let covered = intersection(std::slice::from_ref(&tread), std::slice::from_ref(&slab.outline))
                .map(|r| total_area(&r))
                .unwrap_or(0.0);
            if covered > HEADROOM_OVERLAP {
                lowest = Some(clear);
            }
        }
    }
    lowest
}

/// Checks a railing used as a guard (height, gaps, solid height). An empty list means no remarks.
pub fn railing_checks(doc: &Document, id: Id) -> Result<Vec<Check>, GeometryError> {
    let e = element(doc, id)?;
    let ElementKind::Railing(r) = &e.kind else {
        return Err(GeometryError::WrongKind { id, expected: "railing" });
    };
    let mut out = Vec::new();
    let h = r.height.as_m();
    if h < GUARD_MIN - 1e-9 {
        out.push(Check {
            code: "railing.low",
            level: CheckLevel::Warning,
            value: h,
            limit: GUARD_MIN,
            source: KK_GUARDS,
        });
    }
    // Bars are generated with a clear gap of BAR_GAP (below GUARD_GAP_MAX), so only solid panels need a check.
    if r.infill == RailingInfill::Solid && h > SOLID_GUARD_MAX + 1e-9 {
        out.push(Check {
            code: "railing.solid_high",
            level: CheckLevel::Warning,
            value: h,
            limit: SOLID_GUARD_MAX,
            source: KK_GUARDS,
        });
    }
    Ok(out)
}
