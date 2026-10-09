//! A property form over one or more elements of the same type.
//!
//! Each field reads a value from every selected element. When all agree the value is shown; otherwise the
//! field shows "—". Editing a field sets the value on every element, and [`Form::finish`] turns the edited
//! elements into `element.update` commands that the inspector runs as one undo step.

use egui::Ui;
use kanavos_engine::AnyCommand;
use kanavos_engine::commands::UpdateElement;
use kanavos_model::{ElementKind, Id};
use kanavos_units::{Angle, Length, Locale, Tolerance};

use crate::widgets;

/// A form over the type data `E` of several elements.
pub struct Form<E> {
    /// Element ids and their (possibly edited) type data.
    pub items: Vec<(Id, E)>,
    wrap: fn(E) -> ElementKind,
    edited: bool,
    /// Interface language.
    pub locale: Locale,
    salt: &'static str,
}

impl<E: Clone> Form<E> {
    /// A form over `items`; `wrap` turns the type data back into an [`ElementKind`].
    pub fn new(
        items: Vec<(Id, E)>,
        wrap: fn(E) -> ElementKind,
        locale: Locale,
        salt: &'static str,
    ) -> Form<E> {
        Form { items, wrap, edited: false, locale, salt }
    }

    /// Whether the form covers exactly one element.
    pub fn single(&self) -> Option<&E> {
        match self.items.as_slice() {
            [(_, e)] => Some(e),
            _ => None,
        }
    }

    /// The value shared by every element, or `None` when they differ.
    pub fn shared<T: PartialEq>(&self, get: impl Fn(&E) -> T) -> Option<T> {
        let mut it = self.items.iter().map(|(_, e)| get(e));
        let first = it.next()?;
        if it.all(|v| v == first) { Some(first) } else { None }
    }

    /// The length shared by every element within the project tolerance.
    pub fn shared_length(&self, get: impl Fn(&E) -> Length) -> Option<Length> {
        let tol = Tolerance::default();
        let mut it = self.items.iter().map(|(_, e)| get(e));
        let first = it.next()?;
        if it.all(|v| tol.equal(v, first)) { Some(first) } else { None }
    }

    /// The number shared by every element (within a small epsilon).
    pub fn shared_number(&self, get: impl Fn(&E) -> f64) -> Option<f64> {
        let mut it = self.items.iter().map(|(_, e)| get(e));
        let first = it.next()?;
        if it.all(|v| (v - first).abs() <= 1e-9) { Some(first) } else { None }
    }

    /// Applies a change to every element.
    pub fn apply(&mut self, set: impl Fn(&mut E)) {
        for (_, e) in &mut self.items {
            set(e);
        }
        self.edited = true;
    }

    fn salt(&self, label: &str) -> (&'static str, String) {
        (self.salt, label.to_owned())
    }

    /// A length row.
    pub fn length(
        &mut self,
        ui: &mut Ui,
        label: &str,
        get: impl Fn(&E) -> Length,
        set: impl Fn(&mut E, Length),
    ) {
        widgets::label(ui, label);
        let v = self.shared_length(get);
        if let Some(n) = widgets::length_field(ui, self.salt(label), v, self.locale) {
            self.apply(|e| set(e, n));
        }
        ui.end_row();
    }

    /// A number row with a unit.
    #[allow(clippy::too_many_arguments)]
    pub fn number(
        &mut self,
        ui: &mut Ui,
        label: &str,
        decimals: usize,
        unit: &'static str,
        get: impl Fn(&E) -> f64,
        set: impl Fn(&mut E, f64),
    ) {
        widgets::label(ui, label);
        let v = self.shared_number(get);
        if let Some(n) = widgets::number_field(ui, self.salt(label), v, decimals, unit, self.locale) {
            self.apply(|e| set(e, n));
        }
        ui.end_row();
    }

    /// An angle row in degrees.
    pub fn angle(
        &mut self,
        ui: &mut Ui,
        label: &str,
        get: impl Fn(&E) -> Angle,
        set: impl Fn(&mut E, Angle),
    ) {
        self.number(ui, label, 2, "°", |e| get(e).as_deg(), |e, v| set(e, Angle::deg(v)));
    }

    /// A whole-number row.
    pub fn count(&mut self, ui: &mut Ui, label: &str, get: impl Fn(&E) -> u32, set: impl Fn(&mut E, u32)) {
        self.number(
            ui,
            label,
            0,
            "",
            |e| f64::from(get(e)),
            |e, v| set(e, v.round().clamp(0.0, 10_000.0) as u32),
        );
    }

    /// A check box row.
    pub fn check(&mut self, ui: &mut Ui, label: &str, get: impl Fn(&E) -> bool, set: impl Fn(&mut E, bool)) {
        widgets::label(ui, label);
        let v = self.shared(&get);
        if let Some(n) = widgets::tri_check(ui, v, "") {
            self.apply(|e| set(e, n));
        }
        ui.end_row();
    }

    /// A drop-down row.
    pub fn choice<T: PartialEq + Clone>(
        &mut self,
        ui: &mut Ui,
        label: &str,
        options: &[(T, String)],
        get: impl Fn(&E) -> T,
        set: impl Fn(&mut E, T),
    ) {
        widgets::label(ui, label);
        let v = self.shared(get);
        if let Some(n) = widgets::choice(ui, self.salt(label), v.as_ref(), options, 170.0) {
            self.apply(|e| set(e, n.clone()));
        }
        ui.end_row();
    }

    /// A single-line text row.
    pub fn text(
        &mut self,
        ui: &mut Ui,
        label: &str,
        get: impl Fn(&E) -> String,
        set: impl Fn(&mut E, String),
    ) {
        widgets::label(ui, label);
        let v = self.shared(get);
        if let Some(n) = widgets::text_field(ui, self.salt(label), v.as_deref(), 170.0) {
            self.apply(|e| set(e, n.clone()));
        }
        ui.end_row();
    }

    /// A read-only row.
    pub fn read(&self, ui: &mut Ui, label: &str, value: &str) {
        widgets::label(ui, label);
        widgets::value(ui, value);
        ui.end_row();
    }

    /// The update commands for the edited elements (empty when nothing was edited).
    pub fn finish(self) -> Vec<AnyCommand> {
        if !self.edited {
            return Vec::new();
        }
        let wrap = self.wrap;
        self.items
            .into_iter()
            .map(|(id, e)| UpdateElement { id, kind: Some(wrap(e)), ..UpdateElement::default() }.into())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_geom::Point2;
    use kanavos_model::{ReferenceLine, Structure, Wall};

    fn wall(h: f64) -> Wall {
        Wall {
            start: Point2::new(0.0, 0.0),
            end: Point2::new(4.0, 0.0),
            bulge: 0.0,
            reference: ReferenceLine::Centre,
            structure: Structure::Basic { material: Id(1), thickness: Length::cm(25.0) },
            height: Length::m(h),
            base_offset: Length::ZERO,
        }
    }

    #[test]
    fn shared_values_and_mixed_values() {
        let f = Form::new(vec![(Id(1), wall(3.0)), (Id(2), wall(3.0))], ElementKind::Wall, Locale::El, "t");
        assert!(f.shared_length(|w| w.height).is_some());
        let f = Form::new(vec![(Id(1), wall(3.0)), (Id(2), wall(2.5))], ElementKind::Wall, Locale::El, "t");
        assert!(f.shared_length(|w| w.height).is_none());
        assert_eq!(f.shared(|w| w.reference), Some(ReferenceLine::Centre));
    }

    #[test]
    fn editing_a_mixed_field_updates_every_element() {
        let mut f =
            Form::new(vec![(Id(1), wall(3.0)), (Id(2), wall(2.5))], ElementKind::Wall, Locale::El, "t");
        f.apply(|w| w.height = Length::m(2.8));
        let commands = f.finish();
        assert_eq!(commands.len(), 2);
        for c in commands {
            let AnyCommand::UpdateElement(u) = c else { panic!("not an update") };
            let Some(ElementKind::Wall(w)) = u.kind else { panic!("not a wall") };
            assert!(Tolerance::default().equal(w.height, Length::m(2.8)));
        }
    }

    #[test]
    fn untouched_forms_produce_no_commands() {
        let f = Form::new(vec![(Id(1), wall(3.0))], ElementKind::Wall, Locale::El, "t");
        assert!(f.finish().is_empty());
    }
}
