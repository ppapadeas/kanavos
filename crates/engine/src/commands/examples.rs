//! One example of the arguments of every command, for documentation and clients. Ids refer to the starter
//! template (storeys 28 and 30, plan views 29 and 31, materials 1–18, build-ups 19–27) or to elements a script
//! would have created.

/// `(command name, JSON arguments)` for every command, in [`super::AnyCommand::NAMES`] order.
pub const EXAMPLES: &[(&str, &str)] = &[
    (
        "project.set_info",
        r#"{"info":{"name":"Νέα διώροφη κατοικία","location":"Οδός Λεμονιάς 7","municipality":"Δήμος Δειγματούπολης","owner":"Μαρίνα Δοκιμάκη","designer":"Στέλιος Παραδειγματάκης","date":"Οκτώβριος 2026","revision":"0"}}"#,
    ),
    (
        "site.set",
        r#"{"site":{"plot":[[-4,-6],[16,-6],[16,19],[-4,19]],"street_lines":[[[-4,-6],[16,-6]]],"terms":{"building_coefficient":0.8,"coverage_ratio":0.6,"max_height":7.5,"max_floors":2},"ground_level":-0.3}}"#,
    ),
    ("site.set_plot", r#"{"plot":[[-4,-6],[16,-6],[16,19],[-4,19]]}"#),
    (
        "site.set_terms",
        r#"{"terms":{"building_coefficient":0.8,"coverage_ratio":0.6,"max_height":7.5,"max_floors":2}}"#,
    ),
    (
        "defaults.set",
        r#"{"defaults":{"exterior_wall":19,"interior_wall":21,"floor_slab":24,"ground_slab":25}}"#,
    ),
    ("storey.create", r#"{"name":"Δώμα","elevation":6.4,"height":3.0}"#),
    ("storey.update", r#"{"id":30,"height":3.4,"shift_above":true}"#),
    ("storey.delete", r#"{"id":30}"#),
    ("storey.duplicate", r#"{"id":30,"name":"2ος όροφος"}"#),
    (
        "material.create",
        r#"{"material":{"id":0,"name":"Πέτρα","category":"stone","color":{"r":190,"g":180,"b":160,"a":255},"cut_pattern":"diagonal","priority":50,"conductivity":2.3,"density":2600}}"#,
    ),
    (
        "material.update",
        r#"{"material":{"id":2,"name":"Οπτοπλινθοδομή","category":"masonry","color":{"r":196,"g":122,"b":92,"a":255},"cut_pattern":"masonry","priority":60,"conductivity":0.42}}"#,
    ),
    ("material.delete", r#"{"id":17}"#),
    ("material.duplicate", r#"{"id":1,"name":"Σκυρόδεμα C30/37"}"#),
    (
        "build_up.create",
        r#"{"build_up":{"id":0,"name":"Τοίχος 25 με θερμοπρόσοψη 10","usage":"wall","layers":[{"material":5,"thickness":0.01},{"material":3,"thickness":0.10},{"material":2,"thickness":0.25,"core":true},{"material":5,"thickness":0.02}]}}"#,
    ),
    (
        "build_up.update",
        r#"{"build_up":{"id":21,"name":"Εσωτερικός τοίχος δρομικός 10","usage":"wall","layers":[{"material":5,"thickness":0.02},{"material":2,"thickness":0.10,"core":true},{"material":5,"thickness":0.02}]}}"#,
    ),
    ("build_up.delete", r#"{"id":22}"#),
    ("build_up.duplicate", r#"{"id":19,"name":"Εξωτερικός τοίχος με θερμοπρόσοψη 10"}"#),
    (
        "element.create",
        r#"{"storey":28,"kind":{"type":"wall","start":[0,0],"end":[0,10],"reference":"left","structure":{"type":"layered","build_up":19},"height":3.2},"tag":"Τ1"}"#,
    ),
    ("element.update", r#"{"id":120,"tag":"Θ1","properties":{"κατασκευαστής":"—"}}"#),
    ("element.delete", r#"{"ids":[120,121]}"#),
    ("element.transform", r#"{"ids":[120],"transform":{"type":"move","dx":0,"dy":3},"copy":true,"count":3}"#),
    ("element.copy_to_storey", r#"{"ids":[40,41,42,43],"storey":30}"#),
    ("element.align", r#"{"ids":[60,61,62],"edge":"bottom"}"#),
    ("wall.split", r#"{"id":40,"at":[4.0,0.0]}"#),
    ("wall.join", r#"{"first":40,"second":41}"#),
    (
        "opening.place",
        r#"{"storey":28,"point":[5.0,0.0],"kind":"window","style":"window.casement_2","width":1.2,"height":1.4,"sill":0.9,"tag":"Θ1"}"#,
    ),
    (
        "slab.from_walls",
        r#"{"storey":30,"face":"core","holes":[[[7.665,6.4],[9.835,6.4],[9.835,9.69],[7.665,9.69]]]}"#,
    ),
    ("roof.from_walls", r#"{"storey":30,"kind":"hip","pitch":22,"overhang":0.6}"#),
    (
        "space.detect",
        r#"{"storey":28,"point":[3.0,3.0],"name":"Καθιστικό","number":"0.01","category":"main_use"}"#,
    ),
    ("space.detect_all", r#"{"storey":30,"category":"main_use","locale":"el"}"#),
    ("dimension.auto", r#"{"storey":28,"offset":1.2,"spacing":0.7}"#),
    (
        "view.create",
        r#"{"name":"Τομή Α-Α","kind":{"type":"section","start":[-3,4.2],"end":[15,4.2],"depth":20,"marker":"Α"},"scale":100}"#,
    ),
    ("view.update", r#"{"id":29,"scale":100,"hidden_layers":["object"]}"#),
    ("view.delete", r#"{"id":200}"#),
    ("view.duplicate", r#"{"id":29,"name":"Κάτοψη ισογείου - επίπλωση"}"#),
    (
        "sheet.create",
        r#"{"number":"Α-07","name":"Λεπτομέρειες","paper":{"size":{"type":"a3"},"landscape":true}}"#,
    ),
    (
        "sheet.update",
        r#"{"sheet":{"id":221,"number":"Α-01","name":"Διάγραμμα κάλυψης","paper":{"size":{"type":"a3"},"landscape":true},"viewports":[{"view":34,"centre":[215,163]}]}}"#,
    ),
    ("sheet.delete", r#"{"id":226}"#),
    ("sheet.place_view", r#"{"sheet":221,"view":33,"centre":[120,160],"scale":200}"#),
    ("sheet.move_viewport", r#"{"sheet":225,"index":0,"centre":[120,215]}"#),
    ("sheet.remove_viewport", r#"{"sheet":225,"view":202}"#),
    ("sheet.permit_set", r#"{"locale":"el"}"#),
    (
        "batch",
        r#"{"label":"Περίγραμμα","commands":[{"name":"element.create","args":{"storey":28,"kind":{"type":"column","position":[0.24,0.24],"profile":{"type":"rectangle","width":0.3,"depth":0.3},"material":1,"height":3.2}}},{"name":"element.create","args":{"storey":28,"kind":{"type":"column","position":[11.76,0.24],"profile":{"type":"rectangle","width":0.3,"depth":0.3},"material":1,"height":3.2}}}]}"#,
    ),
];

/// The example arguments of a command.
pub fn example(name: &str) -> Option<serde_json::Value> {
    EXAMPLES.iter().find(|(n, _)| *n == name).and_then(|(_, a)| serde_json::from_str(a).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AnyCommand;

    #[test]
    fn every_command_has_a_valid_example() {
        assert_eq!(EXAMPLES.len(), AnyCommand::NAMES.len());
        for (name, (ex, _)) in AnyCommand::NAMES.iter().zip(EXAMPLES) {
            assert_eq!(name, ex, "examples follow the command order");
            let args = example(name).unwrap_or_else(|| panic!("{name}: example is not JSON"));
            AnyCommand::from_parts(name, args).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
}
