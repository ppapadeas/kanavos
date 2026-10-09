//! Changing parts of the site without replacing it whole.

use kanavos_geom::Polygon2;
use kanavos_model::{BuildingTerms, Document, Site};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Command, CommandError, Outcome};

pub(crate) fn check_terms(t: &BuildingTerms) -> Result<(), CommandError> {
    if !(t.building_coefficient >= 0.0 && t.building_coefficient.is_finite()) {
        return Err(CommandError::InvalidArgument {
            field: "building_coefficient",
            reason: "must be zero or more".into(),
        });
    }
    if !(0.0..=1.0).contains(&t.coverage_ratio) {
        return Err(CommandError::InvalidArgument {
            field: "coverage_ratio",
            reason: "must be between 0 and 1".into(),
        });
    }
    if !(t.max_height.as_m() >= 0.0 && t.max_height.is_finite()) {
        return Err(CommandError::InvalidArgument {
            field: "max_height",
            reason: "must be zero or more".into(),
        });
    }
    if let Some(v) = t.volume_coefficient
        && !(v >= 0.0 && v.is_finite())
    {
        return Err(CommandError::InvalidArgument {
            field: "volume_coefficient",
            reason: "must be zero or more".into(),
        });
    }
    Ok(())
}

/// `site.set_plot` — sets (or removes, with `null`) the plot boundary, keeping the rest of the site.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SetPlot {
    /// Plot boundary in project coordinates, or `null` to remove it.
    pub plot: Option<Polygon2>,
}

impl Command for SetPlot {
    fn name(&self) -> &'static str {
        "site.set_plot"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        if let Some(p) = &self.plot
            && !p.is_valid(&doc.tolerance())
        {
            return Err(CommandError::InvalidArgument {
                field: "plot",
                reason: "needs at least three points enclosing an area".into(),
            });
        }
        let site = Site { plot: self.plot.as_ref().map(Polygon2::to_ccw), ..doc.site().clone() };
        Ok((doc.with_site(site), Outcome::default()))
    }
}

/// `site.set_terms` — sets the building terms (όροι δόμησης), keeping the rest of the site.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SetBuildingTerms {
    /// Building terms.
    pub terms: BuildingTerms,
}

impl Command for SetBuildingTerms {
    fn name(&self) -> &'static str {
        "site.set_terms"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        check_terms(&self.terms)?;
        let site = Site { terms: self.terms.clone(), ..doc.site().clone() };
        Ok((doc.with_site(site), Outcome::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use kanavos_model::starter;
    use kanavos_units::Length;

    #[test]
    fn plot_and_terms_change_independently() {
        let mut s = Session::new(starter());
        let plot = Polygon2::from_m(&[(0.0, 0.0), (0.0, 25.0), (20.0, 25.0), (20.0, 0.0)]);
        s.run(SetPlot { plot: Some(plot) }).unwrap();
        let terms =
            BuildingTerms { max_height: Length::m(7.5), max_floors: Some(2), ..BuildingTerms::default() };
        s.run(SetBuildingTerms { terms: terms.clone() }).unwrap();
        let site = s.document().site();
        assert!(site.plot.as_ref().unwrap().is_ccw());
        assert!((site.plot.as_ref().unwrap().area().as_m2() - 500.0).abs() < 1e-9);
        assert_eq!(site.terms, terms);
        let bad = BuildingTerms { coverage_ratio: 1.5, ..BuildingTerms::default() };
        assert_eq!(s.run(SetBuildingTerms { terms: bad }).unwrap_err().field(), Some("coverage_ratio"));
        let line = Polygon2::from_m(&[(0.0, 0.0), (1.0, 0.0)]);
        assert!(s.run(SetPlot { plot: Some(line) }).is_err());
        s.run(SetPlot { plot: None }).unwrap();
        assert!(s.document().site().plot.is_none());
    }
}
