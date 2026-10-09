//! Design checks that the interface shows next to an element: warnings and notes, never hard errors.
//!
//! Limits for stairs and guards come from the Greek building regulation (Κτιριοδομικός Κανονισμός, Υ.Α.
//! ΥΠΕΝ/ΔΑΟΚΑ/66006/2360, ΦΕΚ Β΄ 3985/22.06.2023): article 11 (stairs, table of §4, types I–III; dwellings may
//! use any type, §4.1) and article 13 (guards). The checks assume type II, the most permissive type allowed for
//! dwellings, and add notes where a stair would not meet type I.

use kanavos_i18n::{Locale, tf};
use kanavos_model::{Document, ElementKind, Id, RailingInfill, StairShape};
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
/// Smallest landing length along the flight axis for a type II stair.
pub const LANDING_MIN_TYPE_II: f64 = 0.90;
/// Smallest guard height.
pub const GUARD_MIN: f64 = 1.00;
/// Largest height of a solid guard.
pub const SOLID_GUARD_MAX: f64 = 1.20;
/// Largest horizontal gap in a guard (for gaps taller than 0.25 m).
pub const GUARD_GAP_MAX: f64 = 0.15;

/// Checks a stair against the comfort rule and the regulation limits. An empty list means no remarks.
///
/// Failure modes: the element is not a stair or its layout cannot be computed.
pub fn stair_checks(doc: &Document, id: Id) -> Result<Vec<Check>, GeometryError> {
    let e = element(doc, id)?;
    let ElementKind::Stair(s) = &e.kind else {
        return Err(GeometryError::WrongKind { id, expected: "stair" });
    };
    let layout = stair_layout(doc, id)?;
    let r = layout.riser_height.as_m();
    let g = s.going.as_m();
    let w = s.width.as_m();
    let mut out = Vec::new();
    let mut push = |code, level, value, limit, source| out.push(Check { code, level, value, limit, source });
    if r > RISER_MAX + 1e-9 {
        push("stair.riser_high", CheckLevel::Warning, r, RISER_MAX, KK_STAIRS);
    } else if r > RISER_MAX_TYPE_I + 1e-9 {
        push("stair.riser_type_ii", CheckLevel::Note, r, RISER_MAX_TYPE_I, KK_STAIRS);
    }
    if r < RISER_MIN - 1e-9 {
        push("stair.riser_low", CheckLevel::Warning, r, RISER_MIN, KK_STAIRS);
    }
    if g < GOING_MIN - 1e-9 {
        push("stair.going_short", CheckLevel::Warning, g, GOING_MIN, KK_STAIRS);
    } else if g < GOING_MIN_TYPE_I - 1e-9 {
        push("stair.going_type_ii", CheckLevel::Note, g, GOING_MIN_TYPE_I, KK_STAIRS);
    }
    let step = 2.0 * r + g;
    if step < STEP_RULE.0 - 1e-9 {
        push("stair.step_rule_low", CheckLevel::Warning, step, STEP_RULE.0, KK_STAIRS);
    } else if step > STEP_RULE.1 + 1e-9 {
        push("stair.step_rule_high", CheckLevel::Warning, step, STEP_RULE.1, KK_STAIRS);
    }
    if w < WIDTH_MIN_DWELLING - 1e-9 {
        push("stair.width_narrow", CheckLevel::Warning, w, WIDTH_MIN_DWELLING, KK_STAIRS_WIDTH);
    } else if w < WIDTH_MIN_TYPE_II - 1e-9 {
        push("stair.width_dwelling_only", CheckLevel::Note, w, WIDTH_MIN_TYPE_II, KK_STAIRS_WIDTH);
    }
    // Rise between landings.
    let flights: Vec<u32> = match s.shape {
        StairShape::Straight => vec![s.risers],
        _ => {
            let first = first_flight_risers(s);
            vec![first, s.risers.saturating_sub(first)]
        }
    };
    let longest = flights.iter().copied().max().unwrap_or(0);
    let flight_rise = r * f64::from(longest);
    if flight_rise > MAX_FLIGHT_RISE + 1e-9 {
        push("stair.flight_rise", CheckLevel::Warning, flight_rise, MAX_FLIGHT_RISE, KK_STAIRS);
    }
    if !layout.landings.is_empty() && w < LANDING_MIN_TYPE_II - 1e-9 {
        push("stair.landing_short", CheckLevel::Warning, w, LANDING_MIN_TYPE_II, KK_STAIRS);
    }
    // The rise must match the risers drawn (the model divides it evenly; flag absurd counts).
    if s.risers > 0 && f64::from(s.risers) * RISER_MIN > s.rise.as_m() + 1e-9 && r < RISER_MIN {
        push(
            "stair.riser_count",
            CheckLevel::Note,
            f64::from(s.risers),
            (s.rise.as_m() / RISER_MIN).floor(),
            KK_STAIRS,
        );
    }
    Ok(out)
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
