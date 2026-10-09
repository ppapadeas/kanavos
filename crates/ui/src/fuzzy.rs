//! Fuzzy, accent-insensitive matching for the command palette (docs/ux.md §3).
//!
//! Both the query and the candidate are folded first: lower case, Greek tonos and dialytika removed, final
//! sigma written as σ. The query is split into words; every query word must match a candidate word as a
//! prefix or, failing that, as an in-order subsequence of one candidate word. So "τοιχ πάχ" finds
//! "Τοίχος · πάχος" and "wll" finds "Wall". Scores favour prefix matches and earlier words.

/// Folds text for matching: lower case, accents removed, final sigma normalised.
pub fn fold(text: &str) -> String {
    text.chars().flat_map(char::to_lowercase).map(fold_char).collect()
}

fn fold_char(c: char) -> char {
    match c {
        'ά' => 'α',
        'έ' => 'ε',
        'ή' => 'η',
        'ί' | 'ϊ' | 'ΐ' => 'ι',
        'ό' => 'ο',
        'ύ' | 'ϋ' | 'ΰ' => 'υ',
        'ώ' => 'ω',
        'ς' => 'σ',
        'à' | 'á' | 'â' | 'ä' => 'a',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ò' | 'ó' | 'ô' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        other => other,
    }
}

fn words(folded: &str) -> Vec<&str> {
    folded.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect()
}

fn is_subsequence(needle: &str, hay: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().all(|n| it.any(|h| h == n))
}

/// Scores a candidate against a query; `None` when it does not match. Higher is better. An empty query
/// matches everything with score 0.
pub fn score(query: &str, candidate: &str) -> Option<i32> {
    let q = fold(query);
    let c = fold(candidate);
    let qwords = words(&q);
    if qwords.is_empty() {
        return Some(0);
    }
    let cwords = words(&c);
    let mut total = 0;
    for qw in &qwords {
        let mut best: Option<i32> = None;
        for (i, cw) in cwords.iter().enumerate() {
            let position_bonus = 10 - (i as i32).min(10);
            let s = if *cw == *qw {
                100 + position_bonus
            } else if cw.starts_with(qw) {
                70 + position_bonus + (qw.chars().count() as i32).min(10)
            } else if cw.contains(qw) {
                40 + position_bonus
            } else if qw.chars().count() >= 2 && is_subsequence(qw, cw) {
                15 + position_bonus
            } else {
                continue;
            };
            best = Some(best.map_or(s, |b| b.max(s)));
        }
        total += best?;
    }
    // Prefer shorter candidates when everything else is equal.
    Some(total * 4 - (cwords.len() as i32).min(40))
}

/// Ranks candidates by score, best first, dropping those that do not match. Ties keep the input order.
pub fn rank<T>(query: &str, items: impl IntoIterator<Item = (T, String)>) -> Vec<(T, i32)> {
    let mut scored: Vec<(usize, T, i32)> = items
        .into_iter()
        .enumerate()
        .filter_map(|(i, (item, text))| score(query, &text).map(|s| (i, item, s)))
        .collect();
    scored.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    scored.into_iter().map(|(_, item, s)| (item, s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folding_removes_greek_accents_and_final_sigma() {
        assert_eq!(fold("Τοίχος"), "τοιχοσ");
        assert_eq!(fold("ΔΙΑΣΤΡΩΜΆΤΩΣΗ"), "διαστρωματωση");
        assert_eq!(fold("Προϊόν ΰ"), "προιον υ");
    }

    #[test]
    fn accent_insensitive_greek_prefixes_match() {
        assert!(score("τοιχ πάχ", "Τοίχος · πάχος").is_some());
        assert!(score("ΤΟΙΧΟΣ", "τοίχος").is_some());
        assert!(score("παραθ", "Πίνακας παραθύρων").is_some());
        assert!(score("κλίμακα", "Σκάλα").is_none());
    }

    #[test]
    fn english_and_subsequences_match() {
        assert!(score("wall", "Wall").is_some());
        assert!(score("wll", "Wall").is_some());
        assert!(score("sched door", "Door schedule").is_some());
        assert!(score("xyz", "Wall").is_none());
    }

    #[test]
    fn every_query_word_must_match() {
        assert!(score("wall slab", "Wall").is_none());
    }

    #[test]
    fn better_matches_rank_first() {
        let items = vec![
            ("schedule", "Πίνακας τοίχων".to_owned()),
            ("tool", "Τοίχος".to_owned()),
            ("other", "Παράθυρο".to_owned()),
        ];
        let ranked = rank("τοιχ", items);
        assert_eq!(ranked.first().map(|r| r.0), Some("tool"));
        assert_eq!(ranked.len(), 2);
    }

    #[test]
    fn empty_query_keeps_order() {
        let ranked = rank("", vec![(1, "b".to_owned()), (2, "a".to_owned())]);
        assert_eq!(ranked.iter().map(|r| r.0).collect::<Vec<_>>(), vec![1, 2]);
    }
}
