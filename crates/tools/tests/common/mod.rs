//! Test harness: feeds scripted events to a tool and applies the commands it returns to a document through
//! `AnyCommand::apply`, as the canvas does, failing the test when a command is rejected.

#![allow(dead_code)]

use std::collections::BTreeSet;

use kanavos_engine::{AnyCommand, Command};
use kanavos_geom::Point2;
use kanavos_model::{Document, Element, ElementKind, Id, ReferenceLine, Structure, Wall, starter};
use kanavos_tools::{
    Key, Modifiers, MouseButton, SnapSettings, Tool, ToolContext, ToolEvent, ToolId, ToolOutput,
    ToolSettings, make_tool,
};
use kanavos_units::{Length, Locale};

pub struct Harness {
    pub doc: Document,
    pub storey: Id,
    pub selection: BTreeSet<Id>,
    pub settings: ToolSettings,
    pub snap: SnapSettings,
    pub pixel: f64,
    pub locale: Locale,
    pub tool: Box<dyn Tool>,
    /// Every command applied, in order.
    pub applied: Vec<AnyCommand>,
    /// The last output.
    pub last: ToolOutput,
}

pub const NONE: Modifiers = Modifiers { shift: false, ctrl: false, alt: false };
pub const SHIFT: Modifiers = Modifiers { shift: true, ctrl: false, alt: false };
pub const CTRL: Modifiers = Modifiers { shift: false, ctrl: true, alt: false };

pub fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

pub fn close(a: Point2, b: Point2) -> bool {
    a.distance(b).as_m() < 1e-6
}

pub fn near_m(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

impl Harness {
    /// A tool on the ground floor of the starter project, snapping off (exact coordinates), 1 px = 1 cm.
    pub fn new(id: ToolId) -> Harness {
        let doc = starter();
        let storey = doc.storeys()[0].id;
        Harness {
            doc,
            storey,
            selection: BTreeSet::new(),
            settings: ToolSettings::default(),
            snap: SnapSettings { enabled: false, ..SnapSettings::default() },
            pixel: 0.01,
            locale: Locale::En,
            tool: make_tool(id),
            applied: Vec::new(),
            last: ToolOutput::default(),
        }
    }

    pub fn with_snapping(mut self) -> Harness {
        self.snap = SnapSettings { grid: None, ..SnapSettings::default() };
        self
    }

    pub fn switch(&mut self, id: ToolId) {
        self.tool = make_tool(id);
    }

    pub fn send(&mut self, ev: ToolEvent) -> ToolOutput {
        let ctx = ToolContext {
            doc: &self.doc,
            storey: self.storey,
            selection: &self.selection,
            pixel_size: self.pixel,
            snap: &self.snap,
            settings: &self.settings,
            locale: self.locale,
        };
        let out = self.tool.handle(&ctx, &ev);
        if let Some(sel) = &out.selection {
            self.selection = sel.clone();
        }
        for c in &out.commands {
            match c.apply(&self.doc) {
                Ok((next, outcome)) => {
                    self.doc = next;
                    for id in outcome.deleted {
                        self.selection.remove(&id);
                    }
                }
                Err(e) => panic!("command {} rejected: {e}\n{c:#?}", c.name()),
            }
            self.applied.push(c.clone());
        }
        self.last = out.clone();
        out
    }

    pub fn move_to(&mut self, x: f64, y: f64) -> ToolOutput {
        self.send(ToolEvent::Move { at: p(x, y), mods: NONE })
    }

    pub fn move_mods(&mut self, x: f64, y: f64, mods: Modifiers) -> ToolOutput {
        self.send(ToolEvent::Move { at: p(x, y), mods })
    }

    /// Move, press and release at a point; returns the merged commands of press and release.
    pub fn click(&mut self, x: f64, y: f64) -> ToolOutput {
        self.click_mods(x, y, NONE)
    }

    pub fn click_mods(&mut self, x: f64, y: f64, mods: Modifiers) -> ToolOutput {
        self.send(ToolEvent::Move { at: p(x, y), mods });
        let mut a = self.send(ToolEvent::Press { at: p(x, y), button: MouseButton::Primary, mods });
        let b = self.send(ToolEvent::Release { at: p(x, y), button: MouseButton::Primary, mods });
        a.commands.extend(b.commands);
        if b.selection.is_some() {
            a.selection = b.selection;
        }
        a.finished |= b.finished;
        a.warning = a.warning.or(b.warning);
        a
    }

    pub fn double_click(&mut self, x: f64, y: f64) -> ToolOutput {
        self.click(x, y);
        self.send(ToolEvent::DoubleClick { at: p(x, y) })
    }

    /// Press at `from`, move in steps to `to`, release there.
    pub fn drag(&mut self, from: (f64, f64), to: (f64, f64), mods: Modifiers) -> ToolOutput {
        self.send(ToolEvent::Move { at: p(from.0, from.1), mods: NONE });
        let mut out =
            self.send(ToolEvent::Press { at: p(from.0, from.1), button: MouseButton::Primary, mods });
        for i in 1..=4 {
            let t = f64::from(i) / 4.0;
            let at = p(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
            self.send(ToolEvent::Move { at, mods });
        }
        let r = self.send(ToolEvent::Release { at: p(to.0, to.1), button: MouseButton::Primary, mods });
        out.commands.extend(r.commands);
        out.warning = out.warning.or(r.warning);
        out
    }

    pub fn key(&mut self, key: Key) -> ToolOutput {
        self.send(ToolEvent::Key { key, mods: NONE })
    }

    pub fn key_mods(&mut self, key: Key, mods: Modifiers) -> ToolOutput {
        self.send(ToolEvent::Key { key, mods })
    }

    pub fn text(&mut self, s: &str) -> ToolOutput {
        self.send(ToolEvent::Text(s.to_owned()))
    }

    /// Elements on the active storey of the given type.
    pub fn of(&self, type_name: &str) -> Vec<Element> {
        self.doc.elements_on(self.storey).filter(|e| e.kind.type_name() == type_name).cloned().collect()
    }

    pub fn walls(&self) -> Vec<Wall> {
        self.doc.walls_on(self.storey).map(|(_, w)| w.clone()).collect()
    }

    /// Adds a wall directly (test setup) and returns its id.
    pub fn add_wall(&mut self, a: (f64, f64), b: (f64, f64)) -> Id {
        let kind = ElementKind::Wall(Wall {
            start: p(a.0, a.1),
            end: p(b.0, b.1),
            bulge: 0.0,
            reference: ReferenceLine::Centre,
            structure: Structure::Layered { build_up: self.doc.defaults().exterior_wall.unwrap() },
            height: Length::m(3.0),
            base_offset: Length::ZERO,
        });
        self.add(kind)
    }

    /// Adds any element on the active storey (test setup).
    pub fn add(&mut self, kind: ElementKind) -> Id {
        let c = kanavos_engine::commands::CreateElement::on(self.storey, kind);
        let (doc, out) = c.apply(&self.doc).unwrap();
        self.doc = doc;
        out.created[0]
    }
}
