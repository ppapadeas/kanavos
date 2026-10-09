//! Quick actions on the selection: the operations offered in the strip next to the selection and in the
//! context menu, and the engine commands they issue. No model logic here: every action is one engine command.

use std::collections::BTreeSet;

use kanavos_drawing::Rect;
use kanavos_engine::AnyCommand;
use kanavos_engine::commands::{
    CopyToStorey, DeleteElements, SplitWall, TransformElements, TransformSpec, UpdateElement,
};
use kanavos_geom::Point2;
use kanavos_model::{Document, ElementKind, Id};

/// An operation on the selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuickAction {
    /// Split the selected wall at the point where it was clicked.
    SplitWall {
        /// The wall.
        wall: Id,
        /// Where it was clicked.
        at: Point2,
    },
    /// Copy the selection to the storey above.
    CopyUp {
        /// That storey.
        storey: Id,
    },
    /// Rotate the selection 90° counter-clockwise about its centre.
    Rotate90,
    /// Mirror the selection about the vertical axis through its centre.
    Mirror,
    /// Copy the selection next to itself.
    Duplicate,
    /// Swap the hinge side of the selected door or window.
    FlipHand(Id),
    /// Swap the side the selected door opens to.
    FlipSide(Id),
    /// Delete the selection.
    Delete,
}

impl QuickAction {
    /// Catalogue key of the button label.
    pub fn key(&self) -> &'static str {
        match self {
            QuickAction::SplitWall { .. } => "ui.canvas.action.split",
            QuickAction::CopyUp { .. } => "ui.canvas.action.copy_up",
            QuickAction::Rotate90 => "ui.canvas.action.rotate",
            QuickAction::Mirror => "ui.canvas.action.mirror",
            QuickAction::Duplicate => "ui.canvas.action.duplicate",
            QuickAction::FlipHand(_) => "ui.canvas.action.flip_hand",
            QuickAction::FlipSide(_) => "ui.canvas.action.flip_side",
            QuickAction::Delete => "ui.canvas.action.delete",
        }
    }

    /// Catalogue key of the tooltip.
    pub fn tip_key(&self) -> String {
        format!("{}.tip", self.key())
    }
}

/// The actions offered for a selection, most likely first (at most six). `clicked` is where the selection was
/// last clicked, for splitting walls.
pub fn quick_actions(
    doc: &Document,
    selection: &BTreeSet<Id>,
    storey: Id,
    clicked: Option<Point2>,
) -> Vec<QuickAction> {
    let mut out = Vec::new();
    let elements: Vec<_> = selection.iter().filter_map(|id| doc.element(*id)).collect();
    if elements.is_empty() {
        return out;
    }
    let above = doc.storey_above(storey).map(|s| s.id);
    let only_openings = elements.iter().all(|e| matches!(e.kind, ElementKind::Opening(_)));
    if let [single] = elements.as_slice() {
        match &single.kind {
            ElementKind::Wall(w) => {
                if let Some(at) = clicked
                    && w.bulge.abs() < 1e-12
                {
                    out.push(QuickAction::SplitWall { wall: single.id, at });
                }
            }
            ElementKind::Opening(o) => {
                out.push(QuickAction::FlipHand(single.id));
                if o.kind != kanavos_model::OpeningKind::Void {
                    out.push(QuickAction::FlipSide(single.id));
                }
            }
            _ => {}
        }
    }
    if !only_openings {
        if let Some(s) = above {
            out.push(QuickAction::CopyUp { storey: s });
        }
        out.push(QuickAction::Rotate90);
        out.push(QuickAction::Mirror);
        out.push(QuickAction::Duplicate);
    }
    out.push(QuickAction::Delete);
    out.truncate(6);
    out
}

/// The commands of an action. `bounds` is the selection's extent in plan (metres), used as the centre for
/// rotating and mirroring and as the offset for duplicating.
pub fn commands(
    action: &QuickAction,
    doc: &Document,
    selection: &BTreeSet<Id>,
    bounds: Option<Rect>,
) -> Vec<AnyCommand> {
    let ids: Vec<Id> = selection.iter().copied().filter(|id| doc.element(*id).is_some()).collect();
    if ids.is_empty() {
        return Vec::new();
    }
    let centre = bounds.map(|b| b.centre()).map(|c| Point2::new(c.x, c.y));
    match action {
        QuickAction::SplitWall { wall, at } => vec![SplitWall { id: *wall, at: *at }.into()],
        QuickAction::CopyUp { storey } => vec![CopyToStorey { ids, storey: *storey }.into()],
        QuickAction::Rotate90 => centre
            .map(|c| {
                TransformElements {
                    ids,
                    transform: TransformSpec::Rotate { centre: c, degrees: 90.0 },
                    copy: false,
                    count: 1,
                }
                .into()
            })
            .into_iter()
            .collect(),
        QuickAction::Mirror => centre
            .map(|c| {
                let [x, y] = c.xy();
                TransformElements {
                    ids,
                    transform: TransformSpec::Mirror {
                        a: Point2::new(x, y - 1.0),
                        b: Point2::new(x, y + 1.0),
                    },
                    copy: false,
                    count: 1,
                }
                .into()
            })
            .into_iter()
            .collect(),
        QuickAction::Duplicate => {
            let dx = bounds.map(|b| b.width() + 0.5).unwrap_or(1.0);
            vec![
                TransformElements {
                    ids,
                    transform: TransformSpec::Move { dx, dy: 0.0 },
                    copy: true,
                    count: 1,
                }
                .into(),
            ]
        }
        QuickAction::FlipHand(id) | QuickAction::FlipSide(id) => {
            let Some(ElementKind::Opening(o)) = doc.element(*id).map(|e| &e.kind) else { return Vec::new() };
            let mut o = o.clone();
            if matches!(action, QuickAction::FlipHand(_)) {
                o.flip_hand = !o.flip_hand;
            } else {
                o.flip_side = !o.flip_side;
            }
            vec![
                UpdateElement { id: *id, kind: Some(ElementKind::Opening(o)), ..UpdateElement::default() }
                    .into(),
            ]
        }
        QuickAction::Delete => vec![DeleteElements { ids }.into()],
    }
}

/// The command copying a selection to a storey (context menu).
pub fn copy_to_storey(doc: &Document, selection: &BTreeSet<Id>, storey: Id) -> Option<AnyCommand> {
    let ids: Vec<Id> = selection.iter().copied().filter(|id| doc.element(*id).is_some()).collect();
    (!ids.is_empty()).then(|| CopyToStorey { ids, storey }.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_engine::Command;

    fn house() -> (Document, Id) {
        let doc = kanavos_model::starter();
        let storey = doc.storeys()[0].id;
        let cmds = crate::canvas::demo::house_commands(&doc, storey, (0.0, 0.0)).unwrap();
        let mut d = doc;
        for c in &cmds {
            d = c.apply(&d).unwrap().0;
        }
        (d, storey)
    }

    #[test]
    fn walls_offer_split_copy_rotate_mirror_and_delete() {
        let (doc, storey) = house();
        let (wall, w) = doc.walls_on(storey).next().unwrap();
        let sel: BTreeSet<Id> = [wall].into();
        let actions = quick_actions(&doc, &sel, storey, Some(w.start.midpoint(w.end)));
        assert!(matches!(actions.first(), Some(QuickAction::SplitWall { .. })));
        assert!(actions.iter().any(|a| matches!(a, QuickAction::CopyUp { .. })));
        assert!(actions.contains(&QuickAction::Delete));
        assert!(actions.len() <= 6);
        // Every action produces a command that applies.
        let bounds = Some(Rect { min: kanavos_drawing::pt(0.0, -0.2), max: kanavos_drawing::pt(10.0, 0.2) });
        for a in &actions {
            let cmds = commands(a, &doc, &sel, bounds);
            assert_eq!(cmds.len(), 1, "{a:?}");
            for c in &cmds {
                c.apply(&doc).unwrap_or_else(|e| panic!("{a:?}: {e}"));
            }
        }
    }

    #[test]
    fn openings_offer_flips() {
        let (doc, storey) = house();
        let door = doc
            .elements_on(storey)
            .find(
                |e| matches!(&e.kind, ElementKind::Opening(o) if o.kind == kanavos_model::OpeningKind::Door),
            )
            .unwrap()
            .id;
        let sel: BTreeSet<Id> = [door].into();
        let actions = quick_actions(&doc, &sel, storey, None);
        assert_eq!(
            actions,
            vec![QuickAction::FlipHand(door), QuickAction::FlipSide(door), QuickAction::Delete]
        );
        let cmds = commands(&QuickAction::FlipHand(door), &doc, &sel, None);
        let (next, _) = cmds[0].apply(&doc).unwrap();
        assert!(next.element(door).and_then(|e| e.as_opening()).unwrap().flip_hand);
    }

    #[test]
    fn nothing_selected_offers_nothing() {
        let (doc, storey) = house();
        assert!(quick_actions(&doc, &BTreeSet::new(), storey, None).is_empty());
    }
}
