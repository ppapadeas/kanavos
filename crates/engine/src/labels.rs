//! History labels with counts and element nouns, e.g. "Διαγραφή 3 στοιχείων" or "Μετακίνηση 2 τοίχων".
//!
//! Catalogue keys (engine.tsv):
//! - `engine.noun.<category>.one` / `.many`: the noun as it follows a verb (Greek genitive), e.g. "τοίχου" /
//!   "τοίχων"; `<category>` is a [`Category::key`] or `element` for mixed selections.
//! - `engine.label.<verb>.one` / `.many`: the pattern, with `{n}` (count) and `{noun}`.
//! - `engine.new.<category>`: the label for creating one element, e.g. "Νέος τοίχος".

use kanavos_i18n::{t, tf};
use kanavos_model::{Category, Document, Id};
use kanavos_units::Locale;

/// The noun key shared by all elements, or `element` when they differ or are unknown.
pub(crate) fn noun_key(doc: Option<&Document>, ids: &[Id]) -> &'static str {
    let Some(doc) = doc else { return "element" };
    let mut cats = ids.iter().filter_map(|id| doc.element(*id)).map(|e| e.category());
    match cats.next() {
        Some(first) if cats.all(|c| c == first) => first.key(),
        _ => "element",
    }
}

/// The noun for `n` items of a noun key.
pub(crate) fn noun(locale: Locale, key: &str, n: usize) -> String {
    let form = if n == 1 { "one" } else { "many" };
    t(locale, &format!("engine.noun.{key}.{form}")).into_owned()
}

/// A counted label, e.g. `counted(El, "delete", 3, "wall")` → "Διαγραφή 3 τοίχων".
pub(crate) fn counted(locale: Locale, verb: &str, n: usize, key: &str) -> String {
    let form = if n == 1 { "one" } else { "many" };
    tf(
        locale,
        &format!("engine.label.{verb}.{form}"),
        &[("n", &n.to_string()), ("noun", &noun(locale, key, n))],
    )
}

/// The label for creating one element of a category, e.g. "Νέος τοίχος".
pub(crate) fn new_element(locale: Locale, category: Category) -> String {
    t(locale, &format!("engine.new.{}", category.key())).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_category_has_nouns_and_a_creation_label() {
        for c in Category::ALL {
            for form in ["one", "many"] {
                assert!(kanavos_i18n::has(&format!("engine.noun.{}.{form}", c.key())), "{c:?} {form}");
            }
            assert!(kanavos_i18n::has(&format!("engine.new.{}", c.key())), "{c:?}");
        }
    }

    #[test]
    fn counts_pick_the_right_form() {
        assert_eq!(counted(Locale::El, "delete", 3, "element"), "Διαγραφή 3 στοιχείων");
        assert_eq!(counted(Locale::El, "move", 2, "wall"), "Μετακίνηση 2 τοίχων");
        assert_eq!(counted(Locale::El, "delete", 1, "wall"), "Διαγραφή τοίχου");
        assert_eq!(counted(Locale::En, "delete", 3, "door"), "Delete 3 doors");
        assert_eq!(counted(Locale::En, "move", 1, "element"), "Move element");
    }
}
