//! Resolved parameter values of an object or opening.

use std::collections::BTreeMap;

use crate::ObjectDef;

/// Parameter values of one placed object: the stored value clamped to the definition's range, or the default.
#[derive(Debug, Clone, Copy)]
pub(crate) struct P<'a> {
    def: &'a ObjectDef,
    values: &'a BTreeMap<String, f64>,
}

impl<'a> P<'a> {
    /// Values for a definition.
    pub(crate) fn new(def: &'a ObjectDef, values: &'a BTreeMap<String, f64>) -> P<'a> {
        P { def, values }
    }

    /// A parameter value; unknown keys give 0.
    pub(crate) fn get(&self, key: &str) -> f64 {
        match self.def.param_def(key) {
            Some(pd) => pd.clamp(self.values.get(key).copied().unwrap_or(pd.default)),
            None => self.values.get(key).copied().filter(|v| v.is_finite()).unwrap_or(0.0),
        }
    }

    /// A whole-number parameter.
    pub(crate) fn count(&self, key: &str) -> usize {
        let v = self.get(key).round();
        if v.is_finite() && v > 0.0 { v as usize } else { 0 }
    }

    /// An on/off parameter.
    pub(crate) fn flag(&self, key: &str) -> bool {
        self.get(key) > 0.5
    }

    /// An angle parameter in radians.
    pub(crate) fn angle(&self, key: &str) -> f64 {
        self.get(key).to_radians()
    }

    /// A percentage parameter as a fraction.
    pub(crate) fn fraction(&self, key: &str) -> f64 {
        (self.get(key) / 100.0).clamp(0.0, 1.0)
    }
}
