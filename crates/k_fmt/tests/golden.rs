//! Golden cases: every `name.k` under `tests/fmt` formats to the sibling
//! `name.formatted.k`, which is itself already formatted. `K_BLESS=1`
//! (re)writes the expectations.

use std::fs;

use crate::inputs;

#[test]
fn cases() {
    let dir = inputs::root().join("tests").join("fmt");
    let bless = std::env::var_os("K_BLESS").is_some();
    let mut failures = Vec::new();
    let mut count = 0;
    for path in inputs::k_files(&dir) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(stem) = name.strip_suffix(".k") else {
            continue;
        };
        if stem.ends_with(".formatted") {
            if !path
                .with_file_name(format!("{}.k", stem.trim_end_matches(".formatted")))
                .exists()
            {
                failures.push(format!("{}: no input for this expectation", path.display()));
            }
            continue;
        }
        count += 1;
        let input = fs::read_to_string(&path).expect("a readable case");
        let expected_path = path.with_file_name(format!("{stem}.formatted.k"));
        let actual = match k_fmt::format(&input) {
            Ok(text) => text,
            Err(e) => {
                failures.push(format!("{}: {e:?}", path.display()));
                continue;
            }
        };
        if bless {
            fs::write(&expected_path, &actual).expect("a writable expectation");
            continue;
        }
        let Ok(expected) = fs::read_to_string(&expected_path) else {
            failures.push(format!(
                "{}: no `{stem}.formatted.k`; run with K_BLESS=1 to write it",
                path.display()
            ));
            continue;
        };
        if actual != expected {
            failures.push(format!(
                "{}:\n{}",
                path.display(),
                crate::oracle::first_difference(&expected, &actual)
            ));
        }
        match k_fmt::format(&expected) {
            Ok(again) if again == expected => {}
            _ => failures.push(format!(
                "{}: the expectation isn't formatted",
                expected_path.display()
            )),
        }
    }
    assert!(count >= 10, "only {count} cases in tests/fmt");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
