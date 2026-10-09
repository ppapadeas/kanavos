//! The ΝΟΚ report computed from a document: coverage, floor area, volume, height, floors, setbacks and
//! per-storey areas by category, with findings for the user.
//!
//! The rules and their sources are in `specs/rules-gr/`; the numbers in [`NokRules`].

use kanavos_elements::base_elevation;
use kanavos_geom::{Point2, PolygonWithHoles};
use kanavos_i18n::tf;
use kanavos_model::{Document, ElementKind, Id, RoofShape, SpaceCategory, Storey, Structure};
use kanavos_units::{
    Area, AreaDisplay, Length, LengthDisplay, Locale, Volume, VolumeDisplay, format_decimal,
};

use crate::footprint::{building_footprint_with, is_enclosed, storey_outline};
use crate::rules::{Counting, NokRules, SetbackHeight};
use crate::setback::{SetbackReport, setbacks};
use crate::{BuildingTerms, Meter, Meters, meters, ops};

/// Areas of one storey.
///
/// Enclosed areas (`gross`, `main`, `auxiliary`, `circulation`, `counted`, `exempt`) are measured to the outer
/// faces of the walls (external insulation excluded): the storey's enclosed outline is shared among its
/// enclosed spaces in proportion to their net areas, so walls count with the spaces they bound. Open areas
/// (`semi_open`, `balconies`, `pilotis`) are the net areas of those spaces.
#[derive(Debug, Clone, PartialEq)]
pub struct StoreyAreas {
    /// Storey.
    pub storey: Id,
    /// Storey name.
    pub name: String,
    /// Floor elevation.
    pub elevation: Length,
    /// Whether the storey is a basement.
    pub below_ground: bool,
    /// Enclosed floor area to the outer faces of the walls (μικτό εμβαδόν ορόφου).
    pub gross: Area,
    /// Main-use spaces (κύριοι χώροι), including spaces of category "other".
    pub main: Area,
    /// Auxiliary spaces (βοηθητικοί χώροι): storage, parking, plant rooms, attics.
    pub auxiliary: Area,
    /// Stairwells, lift shafts and light or ventilation shafts.
    pub circulation: Area,
    /// Semi-open spaces (ημιυπαίθριοι), net.
    pub semi_open: Area,
    /// Balconies (εξώστες), net.
    pub balconies: Area,
    /// Pilotis (πυλωτή), net.
    pub pilotis: Area,
    /// Area counted towards the building coefficient (δόμηση), without open spaces (see
    /// [`NokReport::open_spaces`]).
    pub counted: Area,
    /// Enclosed area exempt from δόμηση.
    pub exempt: Area,
    /// Area this storey adds to coverage (zero below ground).
    pub covered: Area,
    /// Volume above the finished ground (ΝΟΚ 13 [ΚΧΠ 208]): enclosed and semi-open area × floor-to-floor
    /// height for above-ground storeys (with the plinth between ground and floor for the lowest one); for
    /// basements only the part above the ground.
    pub volume: Volume,
}

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Information or an assumption made.
    Info,
    /// Something to check.
    Warning,
    /// A limit of the terms or the code is exceeded.
    Violation,
}

/// A value inserted into a finding's message, formatted for the locale.
#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    /// An area, `12,50 m²`.
    Area(Area),
    /// A length, `3,20 m`.
    Length(Length),
    /// A volume, `412,80 m³`.
    Volume(Volume),
    /// A whole number.
    Count(u32),
    /// Text as is (names).
    Text(String),
}

impl Param {
    /// The value as displayed.
    pub fn display(&self, locale: Locale) -> String {
        match self {
            Param::Area(a) => AreaDisplay(*a, locale).to_string(),
            Param::Length(l) => LengthDisplay(*l, locale).to_string(),
            Param::Volume(v) => VolumeDisplay(*v, locale).to_string(),
            Param::Count(n) => n.to_string(),
            Param::Text(s) => s.clone(),
        }
    }
}

/// A finding of the checks: a catalogue key with parameters, so it can be shown in any language.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Severity.
    pub severity: Severity,
    /// Catalogue key of the message (area `rules`).
    pub key: &'static str,
    /// Values for the message's placeholders.
    pub params: Vec<(&'static str, Param)>,
    /// Where in plan the problem is, if it has a place.
    pub location: Option<Point2>,
    /// The element concerned, if any.
    pub element: Option<Id>,
}

impl Finding {
    fn new(severity: Severity, key: &'static str, params: Vec<(&'static str, Param)>) -> Finding {
        Finding { severity, key, params, location: None, element: None }
    }
    fn at(mut self, p: Option<Point2>) -> Finding {
        self.location = p;
        self
    }
    fn on(mut self, e: Id) -> Finding {
        self.element = Some(e);
        self
    }
    /// The message in a locale.
    pub fn message(&self, locale: Locale) -> String {
        let values: Vec<(&str, String)> = self.params.iter().map(|(k, v)| (*k, v.display(locale))).collect();
        let args: Vec<(&str, &str)> = values.iter().map(|(k, v)| (*k, v.as_str())).collect();
        tf(locale, self.key, &args)
    }
}

/// Open spaces (semi-open spaces and balconies) against their allowance (ΝΟΚ 11 §6 α' [ΚΧΠ 206 §6 α']).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct OpenSpaces {
    /// Net area of above-ground semi-open spaces.
    pub semi_open: Area,
    /// Net area of above-ground balconies.
    pub balconies: Area,
    /// Exempt allowance for both together: [`NokRules::open_space_allowance`] × the allowed floor area.
    pub allowance: Area,
    /// Cap for semi-open spaces alone: [`NokRules::semi_open_cap`] × the allowed floor area.
    pub semi_open_cap: Area,
    /// Area counted towards δόμηση: zero within the allowance, otherwise the whole open area (or the excess,
    /// see [`NokRules::open_excess_counts_whole`]).
    pub counted: Area,
}

/// Number of floors against the limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Floors {
    /// Above-ground storeys with enclosed area (attic-only storeys excluded).
    pub used: u32,
    /// Limit of the terms, if any (the code itself sets none).
    pub allowed: Option<u32>,
}

/// The regulation figures of a project.
#[derive(Debug, Clone, PartialEq)]
pub struct NokReport {
    /// Plot area, if a plot is drawn.
    pub plot_area: Option<Area>,
    /// Coverage and floor-area meters.
    pub meters: Meters,
    /// Building height used and allowed.
    pub height: (Length, Length),
    /// Per-storey areas, lowest first.
    pub storeys: Vec<StoreyAreas>,
    /// Catalogue keys of all findings, for display as notes (see [`NokReport::findings`] for details).
    pub notes: Vec<String>,
    /// The terms the limits come from.
    pub terms: BuildingTerms,
    /// The rule parameters used.
    pub rules: NokRules,
    /// Coverage polygons: the plan projection of the above-ground building.
    pub coverage: Vec<PolygonWithHoles>,
    /// Open spaces against their allowance.
    pub open_spaces: OpenSpaces,
    /// Height meter (same figures as [`NokReport::height`]).
    pub height_meter: Meter<Length>,
    /// Highest point of pitched roofs above the ground, if any.
    pub ridge: Option<Length>,
    /// Volume meter (against the terms' volume coefficient, or 5 × Σ.Δ. by default); `None` without a plot.
    pub volume: Option<Meter<Volume>>,
    /// Volume above the finished ground.
    pub volume_used: Volume,
    /// Number of floors.
    pub floors: Floors,
    /// Setbacks from the plot boundaries.
    pub setbacks: SetbackReport,
    /// Uncovered plot area (ακάλυπτος χώρος).
    pub uncovered: Option<Area>,
    /// Findings, most serious first.
    pub findings: Vec<Finding>,
}

impl NokReport {
    /// Whether every limit is met.
    pub fn complies(&self) -> bool {
        self.findings.iter().all(|f| f.severity != Severity::Violation)
    }
}

/// Thickness of the layers above the first core layer of a structure (insulation and finishes on top), or
/// zero for a basic structure.
fn above_core(doc: &Document, s: &Structure) -> f64 {
    let Some(layers) = doc.structure_layers(s) else { return 0.0 };
    let core = layers.iter().position(|l| l.core).unwrap_or(0);
    layers.iter().take(core).map(|l| l.thickness.as_m()).sum()
}

/// Tops of the above-ground building, as absolute elevations.
#[derive(Debug, Clone, Copy, Default)]
struct Tops {
    /// Top of the structural slab of the last storey (ΝΟΚ 2 §90 [ΚΧΠ 197 αρ. 89]): highest structural top of
    /// flat roofs and slabs, or the lowest bearing point of pitched roofs; walls and columns only when there is
    /// no roof or slab at all.
    building: Option<f64>,
    /// Highest point of pitched roofs.
    ridge: Option<f64>,
    /// Highest wall or column top (rooftop structures included).
    walls: Option<f64>,
}

fn raise(v: &mut Option<f64>, x: f64) {
    *v = Some(v.map_or(x, |t| t.max(x)));
}

fn tops(doc: &Document, rules: &NokRules) -> Tops {
    let mut t = Tops::default();
    let mut slabs: Option<f64> = None;
    for st in doc.storeys().into_iter().filter(|s| !s.below_ground) {
        let floor = st.elevation.as_m();
        for e in doc.elements_on(st.id) {
            match &e.kind {
                ElementKind::Wall(w) if w.height.as_m() >= rules.low_wall_height => {
                    if let Ok(b) = base_elevation(doc, e) {
                        raise(&mut t.walls, b.as_m() + w.height.as_m());
                    }
                }
                ElementKind::Column(c) => {
                    if let Ok(b) = base_elevation(doc, e) {
                        raise(&mut t.walls, b.as_m() + c.height.as_m());
                    }
                }
                ElementKind::Slab(s) => {
                    raise(&mut slabs, floor + s.top_offset.as_m() - above_core(doc, &s.structure));
                }
                ElementKind::Roof(r) => {
                    let eaves = floor + r.base.as_m();
                    match &r.shape {
                        RoofShape::Flat { .. } => raise(&mut slabs, eaves - above_core(doc, &r.structure)),
                        RoofShape::Pitched { slopes } => {
                            let thickness = doc.structure_thickness(&r.structure).map_or(0.0, |l| l.as_m());
                            raise(&mut slabs, eaves - thickness);
                            raise(&mut t.ridge, eaves + roof_rise(&r.outline, slopes));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    t.building = slabs.or(t.walls);
    t
}

/// Rise of a pitched roof above its eaves: the highest point of the lowest roof plane, sampled at the outline's
/// vertices, edge midpoints and centroid (exact for gable, mono-pitch and rectangular hip roofs).
fn roof_rise(outline: &kanavos_geom::Polygon2, slopes: &[Option<kanavos_units::Angle>]) -> f64 {
    let ring = outline.to_ccw();
    let planes: Vec<(kanavos_geom::Segment2, f64)> =
        ring.edges().zip(slopes.iter()).filter_map(|(e, s)| s.map(|a| (e, a.tan()))).collect();
    if planes.is_empty() {
        return 0.0;
    }
    let mut samples: Vec<Point2> = ring.vertices.clone();
    samples.extend(ring.edges().map(|e| e.midpoint()));
    samples.extend(ring.centroid());
    samples
        .iter()
        .map(|p| planes.iter().map(|(e, tan)| e.distance_to(*p).as_m() * tan).fold(f64::INFINITY, f64::min))
        .filter(|h| h.is_finite())
        .fold(0.0, f64::max)
}

struct StoreyContext<'a> {
    below: Option<&'a StoreyAreas>,
    ground: f64,
    lowest_above: bool,
}

fn storey_areas(
    doc: &Document,
    st: &Storey,
    ctx: &StoreyContext<'_>,
    rules: &NokRules,
    findings: &mut Vec<Finding>,
) -> StoreyAreas {
    let outline = storey_outline(doc, st.id, rules);
    let gross = ops::total_area(&outline.enclosed);
    let mut a = StoreyAreas {
        storey: st.id,
        name: st.name.clone(),
        elevation: st.elevation,
        below_ground: st.below_ground,
        gross,
        main: Area::ZERO,
        auxiliary: Area::ZERO,
        circulation: Area::ZERO,
        semi_open: Area::ZERO,
        balconies: Area::ZERO,
        pilotis: Area::ZERO,
        counted: Area::ZERO,
        exempt: Area::ZERO,
        covered: if st.below_ground { Area::ZERO } else { ops::total_area(&outline.covered) },
        volume: Volume::ZERO,
    };
    let mut enclosed: Vec<(Id, SpaceCategory, Area)> = Vec::new();
    for e in doc.elements_on(st.id) {
        let ElementKind::Space(s) = &e.kind else { continue };
        let area = s.boundary.area();
        match s.category {
            SpaceCategory::SemiOpen => a.semi_open += area,
            SpaceCategory::Balcony => a.balconies += area,
            SpaceCategory::Pilotis => a.pilotis += area,
            c if is_enclosed(c) => enclosed.push((e.id, c, area)),
            _ => {}
        }
    }
    let net: f64 = enclosed.iter().map(|(_, _, x)| x.as_m2()).sum();
    if net <= 1e-9 && gross.as_m2() > 1e-9 {
        // Walls without spaces: assume main use above ground, auxiliary below.
        let c = if st.below_ground { SpaceCategory::Auxiliary } else { SpaceCategory::MainUse };
        enclosed.push((st.id, c, gross));
        findings.push(Finding::new(
            Severity::Info,
            if st.below_ground { "rules.note.no_spaces_below" } else { "rules.note.no_spaces" },
            vec![("storey", Param::Text(st.name.clone()))],
        ));
    }
    let net: f64 = enclosed.iter().map(|(_, _, x)| x.as_m2()).sum();
    // Share the enclosed outline among the spaces in proportion to their net areas.
    let k = if net > 1e-9 && gross.as_m2() > 1e-9 { gross.as_m2() / net } else { 1.0 };
    let mut attic = Area::ZERO;
    for (id, c, x) in enclosed {
        let share = x * k;
        match c {
            SpaceCategory::MainUse | SpaceCategory::Other => a.main += share,
            SpaceCategory::Stairwell | SpaceCategory::LiftShaft | SpaceCategory::Shaft => {
                a.circulation += share
            }
            _ => a.auxiliary += share,
        }
        let counted = match rules.counting(c, st.below_ground) {
            Counting::Counted => share,
            Counting::Exempt | Counting::OpenSpaceAllowance => Area::ZERO,
            Counting::Fraction(f) => share * f,
            Counting::StairLimit => {
                let limit = Area::m2(rules.stair_exempt_per_level);
                let excess = (share - limit).max(Area::ZERO);
                if excess.as_m2() > 0.005 {
                    findings.push(
                        Finding::new(
                            Severity::Info,
                            "rules.note.stair_excess",
                            vec![
                                ("storey", Param::Text(st.name.clone())),
                                ("excess", Param::Area(excess)),
                                ("limit", Param::Area(limit)),
                            ],
                        )
                        .on(id),
                    );
                }
                excess
            }
            Counting::ShaftMinimum => {
                if x.as_m2() + 1e-9 < rules.shaft_min_area {
                    share
                } else {
                    Area::ZERO
                }
            }
            Counting::AtticLimit => {
                attic += share;
                Area::ZERO
            }
        };
        a.counted += counted;
        a.exempt += share - counted;
    }
    if attic.as_m2() > 0.0 {
        let limit = ctx.below.map_or(Area::ZERO, |b| b.gross) * rules.attic_exempt_fraction;
        let excess = (attic - limit).max(Area::ZERO);
        if excess.as_m2() > 0.005 {
            findings.push(Finding::new(
                Severity::Info,
                "rules.note.attic_excess",
                vec![("storey", Param::Text(st.name.clone())), ("excess", Param::Area(excess))],
            ));
        }
        a.counted += excess;
        a.exempt -= excess;
    }
    let height = st.height.as_m();
    a.volume = if st.below_ground {
        Volume::m3(gross.as_m2() * (st.top().as_m() - ctx.ground).clamp(0.0, height))
    } else {
        let plinth = if ctx.lowest_above { (st.elevation.as_m() - ctx.ground).max(0.0) } else { 0.0 };
        Volume::m3(gross.as_m2() * (height + plinth) + a.semi_open.as_m2() * height)
    };
    a
}

/// Computes the report with the default rules.
pub fn nok_report(doc: &Document) -> NokReport {
    nok_report_with(doc, &NokRules::default())
}

/// Computes the report with the given rule parameters.
pub fn nok_report_with(doc: &Document, rules: &NokRules) -> NokReport {
    let site = doc.site();
    let mut findings: Vec<Finding> = Vec::new();
    let plot_area = site.plot.as_ref().map(|p| p.area());
    if plot_area.is_none() {
        findings.push(Finding::new(Severity::Warning, "rules.note.no_plot", vec![]));
    }
    let ground = site.ground_level.as_m();
    let allowed_h = if site.terms.max_height.as_m() > 0.0 {
        site.terms.max_height
    } else {
        Length::m(rules.max_height_for(site.terms.building_coefficient))
    };
    let terms = BuildingTerms {
        plot_area: plot_area.unwrap_or(Area::ZERO),
        building_coefficient: site.terms.building_coefficient,
        max_coverage_ratio: site.terms.coverage_ratio,
        volume_coefficient: Some(site.terms.volume_coefficient.unwrap_or_else(|| {
            rules.default_volume_coefficient(site.terms.building_coefficient, allowed_h.as_m())
        })),
    };

    // Per-storey areas, lowest first (attic limits look at the storey below).
    let mut storeys: Vec<StoreyAreas> = Vec::new();
    let mut seen_above = false;
    for st in doc.storeys() {
        let ctx =
            StoreyContext { below: storeys.last(), ground, lowest_above: !st.below_ground && !seen_above };
        seen_above |= !st.below_ground;
        let a = storey_areas(doc, st, &ctx, rules, &mut findings);
        storeys.push(a);
    }

    // Coverage: the projection of every above-ground storey.
    let coverage = building_footprint_with(doc, rules);
    let covered = ops::total_area(&coverage);

    // Open spaces against their allowance.
    let semi: Area = storeys.iter().filter(|s| !s.below_ground).map(|s| s.semi_open).sum();
    let balconies: Area = storeys.iter().filter(|s| !s.below_ground).map(|s| s.balconies).sum();
    let allowance = terms.max_floor_area() * rules.open_space_allowance;
    let cap = terms.max_floor_area() * rules.semi_open_cap;
    let open = semi + balconies;
    let open_counted = if plot_area.is_some() && open.as_m2() > allowance.as_m2() + 0.005 {
        findings.push(Finding::new(
            Severity::Warning,
            "rules.warn.open_space_excess",
            vec![("area", Param::Area(open)), ("allowance", Param::Area(allowance))],
        ));
        if rules.open_excess_counts_whole { open } else { open - allowance }
    } else {
        Area::ZERO
    };
    if plot_area.is_some() && semi.as_m2() > cap.as_m2() + 0.005 {
        findings.push(Finding::new(
            Severity::Violation,
            "rules.warn.semi_open_cap",
            vec![("area", Param::Area(semi)), ("cap", Param::Area(cap))],
        ));
    }
    let open_spaces =
        OpenSpaces { semi_open: semi, balconies, allowance, semi_open_cap: cap, counted: open_counted };
    let mut counted: Vec<Area> = storeys.iter().map(|s| s.counted).collect();
    counted.push(open_counted);
    let mut m = meters(&terms, covered, &counted);
    m.coverage.allowed = Area::m2(rules.allowed_coverage(terms.plot_area.as_m2(), terms.max_coverage_ratio));

    // Height from the finished ground at the building.
    let t = tops(doc, rules);
    let used_h = Length::m(t.building.map_or(0.0, |b| (b - ground).max(0.0)));
    let height_meter = Meter { used: used_h, allowed: allowed_h };
    let ridge = t.ridge.map(|r| Length::m(r - ground));
    if !height_meter.within_limit() {
        findings.push(Finding::new(
            Severity::Violation,
            "rules.warn.height_exceeded",
            vec![("used", Param::Length(used_h)), ("allowed", Param::Length(allowed_h))],
        ));
    }
    if let Some(r) = ridge {
        let limit = allowed_h + Length::m(rules.roof_allowance);
        if r.as_m() > limit.as_m() + 0.005 {
            findings.push(Finding::new(
                Severity::Violation,
                "rules.warn.roof_too_high",
                vec![("used", Param::Length(r)), ("allowed", Param::Length(limit))],
            ));
        }
    }
    if let (Some(b), Some(w)) = (t.building, t.walls) {
        let lift =
            doc.elements().filter_map(|e| e.as_space()).any(|s| s.category == SpaceCategory::LiftShaft);
        let allowance = if lift { rules.housing_allowance_with_lift } else { rules.housing_allowance };
        if w - b > allowance + 0.005 {
            findings.push(Finding::new(
                Severity::Warning,
                "rules.warn.rooftop_too_high",
                vec![
                    ("used", Param::Length(Length::m(w - b))),
                    ("allowed", Param::Length(Length::m(allowance))),
                ],
            ));
        }
    }

    // Basements must have their ceiling close to the ground; pilotis must be large and at ground level.
    for st in doc.storeys().into_iter().filter(|s| s.below_ground) {
        let above = st.top().as_m() - ground;
        if above > rules.basement_max_above_ground + 1e-6 {
            findings.push(Finding::new(
                Severity::Warning,
                "rules.warn.basement_too_high",
                vec![
                    ("storey", Param::Text(st.name.clone())),
                    ("above", Param::Length(Length::m(above))),
                    ("allowed", Param::Length(Length::m(rules.basement_max_above_ground))),
                ],
            ));
        }
    }
    for (st, s) in doc.storeys().into_iter().zip(&storeys) {
        if s.pilotis.as_m2() <= 0.0 {
            continue;
        }
        let small = s.pilotis.as_m2() + 1e-6 < covered.as_m2() * rules.pilotis_min_fraction;
        let off = (st.elevation.as_m() - ground).abs() > rules.pilotis_max_level_difference + 1e-6;
        if small || off {
            findings.push(Finding::new(
                Severity::Warning,
                "rules.warn.pilotis",
                vec![("storey", Param::Text(st.name.clone())), ("area", Param::Area(s.pilotis))],
            ));
        }
    }

    // Floors: above-ground storeys with enclosed area other than attic.
    let attic_only = |s: &StoreyAreas| {
        let mut spaces = doc.elements_on(s.storey).filter_map(|e| e.as_space()).peekable();
        spaces.peek().is_some() && spaces.all(|sp| sp.category == SpaceCategory::Attic)
    };
    let used_floors =
        storeys.iter().filter(|s| !s.below_ground && s.gross.as_m2() > 1e-6 && !attic_only(s)).count() as u32;
    let floors = Floors { used: used_floors, allowed: site.terms.max_floors };
    if let Some(max) = floors.allowed
        && used_floors > max
    {
        findings.push(Finding::new(
            Severity::Violation,
            "rules.warn.floors_exceeded",
            vec![("used", Param::Count(used_floors)), ("allowed", Param::Count(max))],
        ));
    }

    // Volume, with the roof space of pitched roofs (approximated as half the outline area × rise).
    let mut volume_used: Volume = storeys.iter().map(|s| s.volume).sum();
    for st in doc.storeys().into_iter().filter(|s| !s.below_ground) {
        for e in doc.elements_on(st.id) {
            if let ElementKind::Roof(r) = &e.kind
                && let RoofShape::Pitched { slopes } = &r.shape
            {
                volume_used += Volume::m3(r.outline.area().as_m2() * roof_rise(&r.outline, slopes) / 2.0);
            }
        }
    }
    let volume = plot_area.and(terms.max_volume()).map(|allowed| Meter { used: volume_used, allowed });
    if let Some(v) = &volume
        && !v.within_limit()
    {
        findings.push(Finding::new(
            Severity::Violation,
            "rules.warn.volume_exceeded",
            vec![("used", Param::Volume(v.used)), ("allowed", Param::Volume(v.allowed))],
        ));
    }

    if plot_area.is_some() {
        if !m.coverage.within_limit() {
            findings.push(Finding::new(
                Severity::Violation,
                "rules.warn.coverage_exceeded",
                vec![("used", Param::Area(m.coverage.used)), ("allowed", Param::Area(m.coverage.allowed))],
            ));
        }
        if !m.floor_area.within_limit() {
            findings.push(Finding::new(
                Severity::Violation,
                "rules.warn.floor_area_exceeded",
                vec![
                    ("used", Param::Area(m.floor_area.used)),
                    ("allowed", Param::Area(m.floor_area.allowed)),
                ],
            ));
        }
    }

    // Setbacks: H is the built height when the floor area is used up, otherwise the maximum height.
    let exhausted = m.floor_area.allowed.as_m2() > 0.0 && m.floor_area.ratio() >= 0.999;
    let h = match rules.setback_height {
        SetbackHeight::Built => used_h,
        SetbackHeight::Law if exhausted => used_h,
        SetbackHeight::Law => allowed_h,
    };
    let (delta, small_delta) = match site.terms.setback {
        Some(d) => (d, d),
        None => (Length::m(rules.delta_for(h.as_m())), Length::m(rules.small_delta_for(h.as_m()))),
    };
    let sb = setbacks(site, &coverage, delta, small_delta, h);
    for e in sb.edges.iter().filter(|e| !e.ok()) {
        findings.push(
            Finding::new(
                Severity::Violation,
                "rules.warn.setback",
                vec![
                    ("edge", Param::Text(e.name.clone())),
                    ("required", Param::Length(e.required)),
                    ("actual", Param::Length(e.actual.unwrap_or(Length::ZERO))),
                ],
            )
            .at(e.nearest),
        );
    }
    if let Some(plot) = &site.plot {
        let out = ops::difference(&coverage, &[plot.clone().into()]);
        let area = ops::total_area(&out);
        if area.as_m2() > 0.01 {
            let at = out.iter().max_by(|a, b| a.area().total_cmp(&b.area())).and_then(|p| p.outer.centroid());
            findings.push(
                Finding::new(
                    Severity::Violation,
                    "rules.warn.outside_plot",
                    vec![("area", Param::Area(area))],
                )
                .at(at),
            );
        }
    }

    findings.sort_by_key(|f| std::cmp::Reverse(f.severity));
    let notes = findings.iter().map(|f| f.key.to_owned()).collect();
    NokReport {
        plot_area,
        meters: m,
        height: (used_h, allowed_h),
        storeys,
        notes,
        terms,
        rules: rules.clone(),
        coverage,
        open_spaces,
        height_meter,
        ridge,
        volume,
        volume_used,
        floors,
        uncovered: plot_area.map(|p| (p - covered).max(Area::ZERO)),
        setbacks: sb,
        findings,
    }
}

/// A ratio as a percentage with two decimals (e.g. `38,25 %`).
pub fn percent(value: f64, locale: Locale) -> String {
    format!("{} %", format_decimal(value * 100.0, 2, locale))
}
