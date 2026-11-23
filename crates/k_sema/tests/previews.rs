//! `Analysis::previews` (docs/design/57-previews.md): what the tools see of
//! a file's `preview` declarations, the API `k preview` and the editors'
//! panel are built on.

#![allow(clippy::unwrap_used, clippy::panic)]

use k_sema::preview::{Control, ControlValue, Preview};
use k_sema::{AnalysisHost, Options, ProjectPath};

const MANIFEST: &str = "[package]\nname = \"corpus\"\nk = \"0.1\"\npreview = [\"view-previews\"]\n";

const STEPPER: &str = r#"enum Size {
    case small
    case large
}

public component Stepper {
    @Binding var value: Int
    @Prop let label: String = "Count"
    @Prop let size: Size = .small
    @Prop let ratio: Double = 1.0
    @Prop let wait: Duration = 1.seconds

    view {
        "{label}: {value}"
    }
}

preview (value: Int = 1, label: String = "Items", on: Bool = true, size: Size = .large) {
    Stepper(value: bind value, label: label, size: size)
}

preview "at max" (value: Int = -99, ratio: Double = 0.5, wait: Duration = 2.seconds) {
    Stepper(value: bind value)
        .ratio(ratio)
}
"#;

fn previews(text: &str) -> Vec<Preview> {
    let host = AnalysisHost::with_files(
        Options::new(),
        [
            (ProjectPath::new("k.toml").unwrap(), MANIFEST.to_owned()),
            (
                ProjectPath::new("src/ui/Stepper.k").unwrap(),
                text.to_owned(),
            ),
        ],
    );
    let analysis = host.analysis();
    let diagnostics = analysis.diagnostics().unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let file = analysis
        .file_id(&ProjectPath::new("src/ui/Stepper.k").unwrap())
        .unwrap();
    analysis.previews(file).unwrap()
}

#[test]
fn previews_come_in_source_order_with_labels_and_keywords() {
    let list = previews(STEPPER);
    assert_eq!(list.len(), 2);
    // An unnamed preview is labelled after the view it renders.
    assert_eq!(list[0].name, None);
    assert_eq!(list[0].label, "Stepper");
    assert_eq!(list[1].name.as_deref(), Some("at max"));
    assert_eq!(list[1].label, "at max");
    for p in &list {
        let start = usize::from(p.keyword.start());
        assert_eq!(&STEPPER[start..usize::from(p.keyword.end())], "preview");
        assert!(p.range.contains_range(p.keyword));
        assert!(p.view.is_some());
    }
}

#[test]
fn parameters_have_controls_and_literal_defaults() {
    let list = previews(STEPPER);
    let first: Vec<(&str, &Control, Option<&ControlValue>, &str)> = list[0]
        .params
        .iter()
        .map(|p| {
            (
                p.name.as_str(),
                &p.control,
                p.default.as_ref(),
                p.default_source.as_str(),
            )
        })
        .collect();
    assert_eq!(
        first,
        vec![
            ("value", &Control::Integer, Some(&ControlValue::Int(1)), "1"),
            (
                "label",
                &Control::Text,
                Some(&ControlValue::String("Items".into())),
                "\"Items\""
            ),
            (
                "on",
                &Control::Toggle,
                Some(&ControlValue::Bool(true)),
                "true"
            ),
            (
                "size",
                &Control::Choice(vec!["small".into(), "large".into()]),
                Some(&ControlValue::Case("large".into())),
                ".large"
            ),
        ]
    );
    let second = &list[1].params;
    assert_eq!(second[0].default, Some(ControlValue::Int(-99)));
    assert_eq!(second[1].control, Control::Number);
    assert_eq!(second[1].default, Some(ControlValue::Double(0.5)));
    // A type with a literal syntax: a text field; its default isn't a
    // plain literal, so the panel asks the preview for it.
    assert_eq!(second[2].control, Control::Parsed("k.core.Duration".into()));
    assert_eq!(second[2].default, None);
    assert_eq!(second[2].default_source, "2.seconds");
    assert_eq!(second[2].type_name, "Duration");
}

#[test]
fn a_file_without_previews_has_none() {
    let text = "public component Stepper {\n    view {\n        \"x\"\n    }\n}\n";
    assert!(previews(text).is_empty());
}
