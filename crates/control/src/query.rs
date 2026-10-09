//! Queries: elements by category, storey and property; quantities, the ΝΟΚ report and schedules as tables.

use std::collections::BTreeMap;

use kanavos_drawing::table::Table;
use kanavos_geom::Bbox2;
use kanavos_model::{Category, Document, Element, ElementKind, Id, ScheduleKind};
use kanavos_units::Locale;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::rpc::RpcError;

/// A value given as one item or a list.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    /// One.
    One(T),
    /// Several.
    Many(Vec<T>),
}

impl<T: Clone> OneOrMany<T> {
    fn to_vec(&self) -> Vec<T> {
        match self {
            OneOrMany::One(t) => vec![t.clone()],
            OneOrMany::Many(v) => v.clone(),
        }
    }
}

/// A storey by id or by name.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum StoreyRef {
    /// Storey id.
    Id(Id),
    /// Storey name (exact, then case-insensitive).
    Name(String),
}

/// Resolves a storey reference.
pub fn storey(doc: &Document, r: &StoreyRef) -> Result<Id, RpcError> {
    match r {
        StoreyRef::Id(id) => doc.storey(*id).map(|s| s.id),
        StoreyRef::Name(n) => {
            let all = doc.storeys();
            all.iter()
                .find(|s| s.name == *n)
                .or_else(|| all.iter().find(|s| s.name.to_lowercase() == n.to_lowercase()))
                .map(|s| s.id)
        }
    }
    .ok_or_else(|| {
        RpcError::new(crate::rpc::codes::NOT_FOUND, format!("no storey {r:?}"))
            .with_data(json!({ "path": "storey" }))
    })
}

/// Parameters of `query.elements`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ElementQuery {
    /// Category keys, e.g. `"wall"` or `["door", "window"]`.
    pub category: Option<OneOrMany<String>>,
    /// Storey id or name.
    pub storey: Option<StoreyRef>,
    /// Only these elements.
    pub ids: Option<Vec<Id>>,
    /// Exact tag, e.g. `"Θ1"`.
    pub tag: Option<String>,
    /// Name contains (spaces: name; objects and openings: catalogue style; text: content). Case-insensitive.
    pub name: Option<String>,
    /// User properties that must all match exactly.
    pub properties: Option<BTreeMap<String, String>>,
    /// Extra fields to return: `kind`, `properties`, `quantities`, `bbox`, `name`, `storeyName`, `phase`, `view`.
    pub fields: Option<Vec<String>>,
    /// At most this many results.
    pub limit: Option<usize>,
}

const FIELDS: &[&str] = &["kind", "properties", "quantities", "bbox", "name", "storeyName", "phase", "view"];

fn category(key: &str) -> Option<Category> {
    Category::ALL.into_iter().find(|c| c.key() == key)
}

/// Searchable name of an element.
pub fn element_name(e: &Element) -> Option<String> {
    match &e.kind {
        ElementKind::Space(s) => Some(s.name.clone()),
        ElementKind::Object(o) => Some(o.definition.clone()),
        ElementKind::Opening(o) => Some(o.style.clone()),
        ElementKind::Text(t) => Some(t.content.clone()),
        ElementKind::Grid(g) => Some(g.label.clone()),
        ElementKind::Underlay(u) => Some(u.name.clone()),
        ElementKind::Level(l) if !l.label.is_empty() => Some(l.label.clone()),
        _ => None,
    }
}

/// Plan extents of an element.
pub fn extents(doc: &Document, id: Id) -> Option<Bbox2> {
    if let Ok(Some(fp)) = kanavos_elements::footprint(doc, id) {
        return fp.bbox();
    }
    let g = kanavos_elements::snap_geometry(doc, id);
    Bbox2::from_points(
        g.points.iter().map(|(p, _)| *p).chain(g.segments.iter().flat_map(|s| [s.start, s.end])),
    )
}

fn quantities(doc: &Document, id: Id) -> Value {
    match kanavos_elements::quantities(doc, id) {
        Ok(q) => json!({
            "length": q.length.map(|v| v.as_m()),
            "height": q.height.map(|v| v.as_m()),
            "thickness": q.thickness.map(|v| v.as_m()),
            "area": q.area.map(|v| v.as_m2()),
            "grossArea": q.gross_area.map(|v| v.as_m2()),
            "perimeter": q.perimeter.map(|v| v.as_m()),
            "volume": q.volume.map(|v| v.as_m3()),
        }),
        Err(e) => json!({ "error": e.to_string() }),
    }
}

/// Runs an element query.
pub fn elements(doc: &Document, q: &ElementQuery) -> Result<Value, RpcError> {
    let cats: Option<Vec<Category>> = match &q.category {
        None => None,
        Some(c) => {
            let mut out = Vec::new();
            for (i, key) in c.to_vec().iter().enumerate() {
                let cat = category(key).ok_or_else(|| {
                    let keys: Vec<&str> = Category::ALL.iter().map(|c| c.key()).collect();
                    RpcError::invalid_params(
                        &format!("category[{i}]"),
                        format!("unknown category '{key}'; one of {}", keys.join(", ")),
                    )
                })?;
                out.push(cat);
            }
            Some(out)
        }
    };
    let fields = q.fields.clone().unwrap_or_default();
    for (i, f) in fields.iter().enumerate() {
        if !FIELDS.contains(&f.as_str()) {
            return Err(RpcError::invalid_params(
                &format!("fields[{i}]"),
                format!("unknown field '{f}'; one of {}", FIELDS.join(", ")),
            ));
        }
    }
    let storey = q.storey.as_ref().map(|s| storey(doc, s)).transpose()?;
    let name = q.name.as_ref().map(|n| n.to_lowercase());
    let limit = q.limit.unwrap_or(usize::MAX);
    let mut out = Vec::new();
    let mut total = 0usize;
    for e in doc.elements() {
        if let Some(ids) = &q.ids
            && !ids.contains(&e.id)
        {
            continue;
        }
        if let Some(c) = &cats
            && !c.contains(&e.category())
        {
            continue;
        }
        if storey.is_some() && e.storey != storey {
            continue;
        }
        if let Some(t) = &q.tag
            && e.tag != *t
        {
            continue;
        }
        if let Some(n) = &name
            && !element_name(e).is_some_and(|s| s.to_lowercase().contains(n.as_str()))
        {
            continue;
        }
        if let Some(props) = &q.properties
            && !props.iter().all(|(k, v)| e.properties.get(k) == Some(v))
        {
            continue;
        }
        total += 1;
        if out.len() >= limit {
            continue;
        }
        let mut m = Map::new();
        m.insert("id".into(), json!(e.id));
        m.insert("category".into(), json!(e.category().key()));
        m.insert("type".into(), json!(e.kind.type_name()));
        m.insert("storey".into(), json!(e.storey));
        m.insert("tag".into(), json!(e.tag));
        for f in &fields {
            let v = match f.as_str() {
                "kind" => serde_json::to_value(&e.kind).unwrap_or(Value::Null),
                "properties" => json!(e.properties),
                "quantities" => quantities(doc, e.id),
                "bbox" => extents(doc, e.id).map(|b| json!([b.min, b.max])).unwrap_or(Value::Null),
                "name" => json!(element_name(e)),
                "storeyName" => json!(e.storey.and_then(|s| doc.storey(s)).map(|s| s.name.clone())),
                "phase" => json!(e.phase),
                "view" => json!(e.view),
                _ => Value::Null,
            };
            m.insert(f.clone(), v);
        }
        out.push(Value::Object(m));
    }
    Ok(json!({ "count": total, "elements": out }))
}

/// A table as JSON: title, headers, rows and footer as displayed text, and CSV.
pub fn table_json(t: &Table, locale: Locale) -> Value {
    let text = |rows: &Vec<Vec<kanavos_drawing::table::Cell>>| -> Vec<Vec<String>> {
        rows.iter().map(|r| r.iter().map(|c| c.display(locale)).collect()).collect()
    };
    json!({
        "title": t.title,
        "columns": t.columns.iter().map(|c| c.header.clone()).collect::<Vec<_>>(),
        "rows": text(&t.rows),
        "footer": text(&t.footer),
        "csv": t.to_csv(locale),
    })
}

/// The ΝΟΚ report as JSON (areas in m², lengths in m).
pub fn nok_json(doc: &Document, locale: Locale) -> Value {
    let r = kanavos_rules_gr::nok_report(doc);
    let meter = |m: &kanavos_rules_gr::Meter| {
        let ratio = m.ratio();
        json!({
            "used": m.used.as_m2(),
            "allowed": m.allowed.as_m2(),
            "margin": m.margin().as_m2(),
            "ratio": if ratio.is_finite() { json!(ratio) } else { Value::Null },
            "withinLimit": m.within_limit(),
            "label": m.label(locale),
        })
    };
    json!({
        "plotArea": r.plot_area.map(|a| a.as_m2()),
        "coverage": meter(&r.meters.coverage),
        "floorArea": meter(&r.meters.floor_area),
        "height": { "used": r.height.0.as_m(), "allowed": r.height.1.as_m(),
                    "withinLimit": r.height.0.as_m() <= r.height.1.as_m() },
        "storeys": r.storeys.iter().map(|s| json!({
            "storey": s.storey, "name": s.name, "gross": s.gross.as_m2(),
            "counted": s.counted.as_m2(), "exempt": s.exempt.as_m2(),
        })).collect::<Vec<_>>(),
        "notes": r.notes.iter().map(|k| kanavos_i18n::t(locale, k).into_owned()).collect::<Vec<_>>(),
    })
}

/// Totals by category: counts, and lengths, areas and volumes where they apply.
pub fn totals_json(doc: &Document) -> Value {
    #[derive(Default)]
    struct Sum {
        count: usize,
        length: f64,
        area: f64,
        volume: f64,
    }
    let mut sums: BTreeMap<&'static str, Sum> = BTreeMap::new();
    for e in doc.elements() {
        let s = sums.entry(e.category().key()).or_default();
        s.count += 1;
        if let Ok(q) = kanavos_elements::quantities(doc, e.id) {
            s.length += q.length.map(|v| v.as_m()).unwrap_or(0.0);
            s.area += q.area.map(|v| v.as_m2()).unwrap_or(0.0);
            s.volume += q.volume.map(|v| v.as_m3()).unwrap_or(0.0);
        }
    }
    let mut m = Map::new();
    for (k, s) in sums {
        m.insert(
            k.into(),
            json!({ "count": s.count, "length": s.length, "area": s.area, "volume": s.volume }),
        );
    }
    json!({ "categories": m, "elements": doc.element_count() })
}

/// Parameters of `query.quantities`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuantityQuery {
    /// `nok`, `schedule`, `area_table` or `totals`.
    pub what: String,
    /// Which schedule for `schedule`: doors, windows, spaces, walls, slabs, materials, objects.
    #[serde(default)]
    pub schedule: Option<ScheduleKind>,
    /// Language of headers and notes.
    #[serde(default)]
    pub locale: Option<Locale>,
}

/// Runs a quantity query.
pub fn quantities_query(
    doc: &Document,
    q: &QuantityQuery,
    default_locale: Locale,
) -> Result<Value, RpcError> {
    let locale = q.locale.unwrap_or(default_locale);
    match q.what.as_str() {
        "nok" => Ok(nok_json(doc, locale)),
        "area_table" => Ok(table_json(&kanavos_rules_gr::area_table(doc, locale), locale)),
        "schedule" => {
            let kind = q.schedule.ok_or_else(|| RpcError::invalid_params("schedule", "is required"))?;
            Ok(table_json(&kanavos_schedules::schedule(doc, kind, locale), locale))
        }
        "totals" => Ok(totals_json(doc)),
        other => Err(RpcError::invalid_params(
            "what",
            format!("unknown '{other}'; one of nok, schedule, area_table, totals"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Document {
        kanavos_engine::sample::sample_house().unwrap()
    }

    #[test]
    fn elements_filter_by_category_storey_and_tag() {
        let doc = sample();
        let q = ElementQuery {
            category: Some(OneOrMany::Many(vec!["door".into(), "window".into()])),
            storey: Some(StoreyRef::Name("1ος όροφος".into())),
            tag: Some("Θ2".into()),
            fields: Some(vec!["quantities".into(), "storeyName".into()]),
            ..ElementQuery::default()
        };
        let r = elements(&doc, &q).unwrap();
        assert_eq!(r["count"], 4);
        let first = &r["elements"][0];
        assert_eq!(first["category"], "window");
        assert_eq!(first["storeyName"], "1ος όροφος");
        assert!((first["quantities"]["area"].as_f64().unwrap() - 0.96).abs() < 1e-9);
    }

    #[test]
    fn unknown_fields_and_categories_name_their_path() {
        let doc = sample();
        let q = ElementQuery { category: Some(OneOrMany::One("walls".into())), ..ElementQuery::default() };
        let e = elements(&doc, &q).unwrap_err();
        assert_eq!(e.data.unwrap()["path"], "category[0]");
        let q =
            ElementQuery { fields: Some(vec!["kind".into(), "colour".into()]), ..ElementQuery::default() };
        assert_eq!(elements(&doc, &q).unwrap_err().data.unwrap()["path"], "fields[1]");
    }

    #[test]
    fn spaces_are_found_by_name() {
        let doc = sample();
        let q = ElementQuery {
            name: Some("υπνοδωμάτιο".into()), limit: Some(2), ..ElementQuery::default()
        };
        let r = elements(&doc, &q).unwrap();
        assert_eq!(r["count"], 3);
        assert_eq!(r["elements"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn quantities_cover_nok_tables_and_totals() {
        let doc = sample();
        let nok = quantities_query(
            &doc,
            &QuantityQuery { what: "nok".into(), schedule: None, locale: None },
            Locale::El,
        )
        .unwrap();
        assert!((nok["plotArea"].as_f64().unwrap() - 500.0).abs() < 1e-6);
        assert_eq!(nok["coverage"]["withinLimit"], true);
        let doors = quantities_query(
            &doc,
            &QuantityQuery { what: "schedule".into(), schedule: Some(ScheduleKind::Doors), locale: None },
            Locale::En,
        )
        .unwrap();
        assert!(doors["rows"].as_array().unwrap().len() >= 4);
        assert!(doors["csv"].as_str().unwrap().contains("Π4"));
        let totals = quantities_query(
            &doc,
            &QuantityQuery { what: "totals".into(), schedule: None, locale: None },
            Locale::El,
        )
        .unwrap();
        assert_eq!(totals["categories"]["wall"]["count"], 24);
    }
}
