//! Attributes shared by elements: materials and build-ups.

use kanavos_drawing::{HatchPattern, Rgba};
use kanavos_geom::Length;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Id;

/// Broad family of a material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MaterialCategory {
    /// Reinforced or plain concrete.
    Concrete,
    /// Brick and block masonry.
    Masonry,
    /// Thermal and acoustic insulation.
    Insulation,
    /// Plaster and render.
    Plaster,
    /// Natural stone.
    Stone,
    /// Ceramic and stone tiles.
    Tile,
    /// Timber and boards.
    Wood,
    /// Steel, aluminium.
    Metal,
    /// Glass.
    Glass,
    /// Waterproofing and vapour membranes.
    Membrane,
    /// Screeds and mortar beds.
    Screed,
    /// Gravel and aggregates.
    Gravel,
    /// Earth and fill.
    Soil,
    /// Air gaps.
    Air,
    /// Gypsum board and other dry linings.
    Board,
    /// Anything else.
    #[default]
    Other,
}

/// A physical material with its drawing appearance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Material {
    /// Identifier.
    pub id: Id,
    /// Display name, e.g. "Οπτοπλινθοδομή".
    pub name: String,
    /// Family.
    #[serde(default)]
    pub category: MaterialCategory,
    /// Surface colour in 3D views and elevations.
    pub color: Rgba,
    /// Hatch where the material is cut in plans and sections.
    #[serde(default)]
    pub cut_pattern: HatchPattern,
    /// Background colour of cut areas, if any.
    #[serde(default)]
    pub cut_background: Option<Rgba>,
    /// Junction priority: where elements meet, higher-priority materials run through lower ones.
    #[serde(default)]
    pub priority: u8,
    /// Thermal conductivity λ, W/(m·K).
    #[serde(default)]
    pub conductivity: Option<f64>,
    /// Density, kg/m³.
    #[serde(default)]
    pub density: Option<f64>,
}

/// What a build-up is meant for (used to filter choices in the interface).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BuildUpUsage {
    /// Walls.
    Wall,
    /// Slabs.
    Slab,
    /// Roofs.
    Roof,
    /// Any element.
    #[default]
    Any,
}

/// One layer of a build-up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BuildUpLayer {
    /// Material.
    pub material: Id,
    /// Thickness.
    pub thickness: Length,
    /// Whether the layer is part of the structural core (used for dimensioning and junctions).
    #[serde(default)]
    pub core: bool,
}

/// An ordered set of layers (διαστρωμάτωση). Walls list layers left face → right face; slabs and roofs top →
/// bottom.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BuildUp {
    /// Identifier.
    pub id: Id,
    /// Display name, e.g. "Εξωτερικός τοίχος 25 με θερμοπρόσοψη".
    pub name: String,
    /// Intended use.
    #[serde(default)]
    pub usage: BuildUpUsage,
    /// Layers in order.
    pub layers: Vec<BuildUpLayer>,
}

impl BuildUp {
    /// Total thickness.
    pub fn thickness(&self) -> Length {
        self.layers.iter().map(|l| l.thickness).sum()
    }

    /// Thermal transmittance U in W/(m²·K) by ISO 6946 (layers in series, Rsi + Rse for the usage), or `None`
    /// when any non-air layer lacks a conductivity. `lookup` returns a material's conductivity.
    pub fn u_value(&self, lookup: impl Fn(Id) -> Option<f64>) -> Option<f64> {
        let (rsi, rse) = match self.usage {
            BuildUpUsage::Roof => (0.10, 0.04),
            BuildUpUsage::Slab => (0.17, 0.04),
            _ => (0.13, 0.04),
        };
        let mut r = rsi + rse;
        for l in &self.layers {
            let lambda = lookup(l.material)?;
            if lambda <= 0.0 {
                return None;
            }
            r += l.thickness.as_m() / lambda;
        }
        if r > 0.0 { Some(1.0 / r) } else { None }
    }
}
