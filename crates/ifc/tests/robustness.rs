//! Property tests: text survives the Part 21 encoding, numbers keep their value, and neither the reader nor the
//! importer panics on damaged input.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kanavos_ifc::step::{encode_string, format_real, parse};
use kanavos_ifc::{ImportOptions, import};
use proptest::prelude::*;

fn wrap(params: &str) -> String {
    format!(
        "ISO-10303-21;HEADER;FILE_SCHEMA(('IFC4'));ENDSEC;DATA;\n#1=IFCTEST({params});\nENDSEC;END-ISO-10303-21;"
    )
}

const FIXTURE: &str = include_str!("data/house-ifc4.ifc");

proptest! {
    #[test]
    fn any_text_survives_encoding(text in prop::collection::vec(any::<char>(), 0..40).prop_map(String::from_iter)) {
        let encoded = encode_string(&text);
        prop_assert!(encoded.is_ascii());
        let file = parse(wrap(&encoded).as_bytes()).unwrap();
        prop_assert_eq!(file.get(1).unwrap().str_at(0), Some(text.as_str()));
    }

    #[test]
    fn reals_keep_their_value(v in any::<f64>().prop_filter("finite", |v| v.is_finite())) {
        let file = parse(wrap(&format_real(v)).as_bytes()).unwrap();
        let back = file.get(1).unwrap().f64_at(0).unwrap();
        if v.abs() < 1e-12 {
            prop_assert!(back.abs() < 1e-300);
        } else {
            prop_assert!((back - v).abs() <= 1e-11 * v.abs().max(1.0), "{} -> {} -> {}", v, format_real(v), back);
        }
    }

    #[test]
    fn the_reader_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..400)) {
        let _ = parse(&bytes);
        let mut text = b"ISO-10303-21;HEADER;ENDSEC;DATA;#1=".to_vec();
        text.extend_from_slice(&bytes);
        let _ = parse(&text);
    }

    #[test]
    fn the_importer_never_panics_on_damaged_files(
        edits in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>(), 0u8..3), 1..8)
    ) {
        let mut bytes = FIXTURE.as_bytes().to_vec();
        for (at, byte, op) in edits {
            if bytes.is_empty() {
                break;
            }
            let i = at.index(bytes.len());
            match op {
                0 => bytes[i] = byte,
                1 => {
                    bytes.remove(i);
                }
                _ => bytes.insert(i, byte),
            }
        }
        if let Ok(imported) = import(&bytes, &ImportOptions::default()) {
            prop_assert!(imported.document.validate().is_ok());
        }
    }

    /// Syntactically valid files whose references point at the wrong kinds of entities.
    #[test]
    fn the_importer_never_panics_on_wrong_references(
        swaps in prop::collection::vec((any::<prop::sample::Index>(), 1u64..170), 1..12)
    ) {
        let mut text = FIXTURE.to_owned();
        for (at, target) in swaps {
            let refs: Vec<(usize, usize)> = text
                .match_indices('#')
                .filter_map(|(i, _)| {
                    let digits = text[i + 1..].chars().take_while(char::is_ascii_digit).count();
                    // Only references inside parameter lists, not instance names at the start of a line.
                    (digits > 0 && i > 0 && &text[i - 1..i] != "\n").then_some((i, i + 1 + digits))
                })
                .collect();
            if refs.is_empty() {
                break;
            }
            let (start, end) = refs[at.index(refs.len())];
            text.replace_range(start..end, &format!("#{target}"));
        }
        if let Ok(imported) = import(text.as_bytes(), &ImportOptions::default()) {
            prop_assert!(imported.document.validate().is_ok());
        }
    }
}

#[test]
fn reference_cycles_terminate() {
    // A placement placed relative to itself, and a mapped item that maps itself.
    let text = "ISO-10303-21;HEADER;FILE_SCHEMA(('IFC4'));ENDSEC;DATA;\n\
        #1=IFCPROJECT('3aProject0000000000001',$,'P',$,$,$,$,$,$);\n\
        #2=IFCBUILDINGSTOREY('3aStorey00000000000001',$,'S',$,$,#3,$,$,.ELEMENT.,0.);\n\
        #3=IFCLOCALPLACEMENT(#3,$);\n\
        #4=IFCMAPPEDITEM(#5,$);\n#5=IFCREPRESENTATIONMAP($,#6);\n\
        #6=IFCSHAPEREPRESENTATION($,'Body','MappedRepresentation',(#4));\n\
        #7=IFCPRODUCTDEFINITIONSHAPE($,$,(#6));\n\
        #8=IFCWALL('3aWall0000000000000001',$,'W',$,$,#3,#7,$,$);\n\
        ENDSEC;END-ISO-10303-21;";
    let imported = import(text.as_bytes(), &ImportOptions::default()).unwrap();
    assert_eq!(imported.report.skipped["IFCWALL"].values().sum::<usize>(), 1);
}
