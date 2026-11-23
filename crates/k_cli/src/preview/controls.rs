//! The panel's control values (docs/compiler/preview.md, "Control
//! values"): how a parameter's control and value are written in the JSON
//! the panel and the frame exchange, how a value the panel sends is
//! checked against its control, and which values a rebuild keeps.
//!
//! A value is the JSON of what the control holds: `true`/`false` for a
//! toggle, a number for `Int` (an integer) and `Double`, a string for a
//! text field, the case's name for an enum, and the text for a type with
//! a literal syntax (`"24.00.usd"`, `"5.minutes"`, `"2026-01-01"`), which
//! the frame parses with the type's parser. `null` is the parameter's
//! default.

use std::collections::BTreeMap;

use k_sema::preview::{Control, ControlValue, PreviewParam};
use serde_json::{Value, json};

/// The control of a parameter as the panel reads it:
/// `{"kind": "toggle"|"integer"|"number"|"text"|"choice"|"parsed", "cases"?, "type"?}`.
#[must_use]
pub(crate) fn control_json(control: &Control) -> Value {
    match control {
        Control::Toggle => json!({ "kind": "toggle" }),
        Control::Integer => json!({ "kind": "integer" }),
        Control::Number => json!({ "kind": "number" }),
        Control::Text => json!({ "kind": "text" }),
        Control::Choice(cases) => json!({ "kind": "choice", "cases": cases }),
        Control::Parsed(ty) => json!({ "kind": "parsed", "type": ty }),
        Control::Unsupported => json!({ "kind": "unsupported" }),
    }
}

/// The default a control shows before anyone touches it, or `null` when
/// the default isn't a literal the panel can show (the control then shows
/// the default's source as its placeholder).
#[must_use]
pub(crate) fn default_json(p: &PreviewParam) -> Value {
    match (&p.control, &p.default) {
        (Control::Parsed(ty), _) => {
            parsed_default(ty, &p.default_source).map_or(Value::Null, Value::String)
        }
        (_, Some(ControlValue::Bool(b))) => json!(b),
        (_, Some(ControlValue::Int(i))) => json!(i),
        (_, Some(ControlValue::Double(x))) => json!(x),
        (_, Some(ControlValue::String(s))) => json!(s),
        (_, Some(ControlValue::Case(c))) => json!(c),
        (_, None) => Value::Null,
    }
}

/// The text a parsed control starts with: a `Money` or `Duration` default
/// written as a literal (`10.eur`, `5.minutes`) is that text; a default
/// `T.parse("…")` (with `!` or `?? …` after it) is the string.
fn parsed_default(ty: &str, source: &str) -> Option<String> {
    let s = source.trim();
    if ty == "k.core.Money" || ty == "k.core.Duration" {
        let literal = s.split_once('.').is_some_and(|(n, rest)| {
            n.trim_start_matches('-')
                .chars()
                .all(|c| c.is_ascii_digit() || c == '_')
                && !n.is_empty()
                && rest
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
        });
        return literal.then(|| s.to_owned());
    }
    let open = s.find(".parse(\"")? + ".parse(\"".len();
    let rest = &s[open..];
    let close = rest.find("\")")?;
    let text = &rest[..close];
    (!text.contains('\\') && !text.contains('{')).then(|| text.to_owned())
}

/// Checks a value the panel sent for `control`: the value itself when it
/// fits, else what is wrong with it. `null` (the default) always fits.
///
/// # Errors
///
/// A message naming what the control expects.
pub(crate) fn decode(control: &Control, v: &Value) -> Result<Value, String> {
    if v.is_null() {
        return Ok(Value::Null);
    }
    let ok = match control {
        Control::Toggle => v.is_boolean(),
        Control::Integer => v
            .as_f64()
            .is_some_and(|x| x.fract() == 0.0 && x.abs() <= 9_007_199_254_740_991.0),
        Control::Number => v.as_f64().is_some_and(f64::is_finite),
        Control::Text | Control::Parsed(_) => v.is_string(),
        Control::Choice(cases) => v.as_str().is_some_and(|s| cases.iter().any(|c| c == s)),
        Control::Unsupported => false,
    };
    if !ok {
        return Err(match control {
            Control::Toggle => "a toggle holds `true` or `false`".to_owned(),
            Control::Integer => "an `Int` control holds a whole number".to_owned(),
            Control::Number => "a `Double` control holds a finite number".to_owned(),
            Control::Text => "a text field holds a string".to_owned(),
            Control::Parsed(ty) => format!("a `{ty}` control holds its text"),
            Control::Choice(cases) => format!("this control holds one of {}", cases.join(", ")),
            Control::Unsupported => "this parameter has no control".to_owned(),
        });
    }
    // An `Int` is sent as a whole number even when it was written `3.0`.
    if *control == Control::Integer
        && let Some(x) = v.as_f64()
    {
        #[allow(clippy::cast_possible_truncation)]
        return Ok(json!(x as i64));
    }
    Ok(v.clone())
}

/// A value the panel set for a parameter, with the parameter's type when
/// it was set, which a rebuild compares.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Kept {
    /// The parameter's type as written (`Int`, `Tone`).
    pub(crate) type_name: String,
    /// The value, never `null` (a default isn't kept).
    pub(crate) value: Value,
}

/// The values a rebuild keeps for a preview whose parameters are now
/// `params` (57-previews.md, "`k preview`"): by parameter name and type,
/// so a renamed or retyped parameter goes back to its default, and only
/// values its control still accepts (a removed enum case is dropped).
#[must_use]
pub(crate) fn retain(
    kept: &BTreeMap<String, Kept>,
    params: &[PreviewParam],
) -> BTreeMap<String, Kept> {
    let mut out = BTreeMap::new();
    for p in params {
        let Some(k) = kept.get(&p.name) else { continue };
        if k.type_name != p.type_name {
            continue;
        }
        if let Ok(v) = decode(&p.control, &k.value)
            && !v.is_null()
        {
            out.insert(
                p.name.clone(),
                Kept {
                    type_name: k.type_name.clone(),
                    value: v,
                },
            );
        }
    }
    out
}

/// The values the panel shows for `params`: each kept value, else the
/// default ([`default_json`]).
#[must_use]
pub(crate) fn shown(
    kept: &BTreeMap<String, Kept>,
    params: &[PreviewParam],
) -> BTreeMap<String, Value> {
    params
        .iter()
        .map(|p| {
            let v = kept
                .get(&p.name)
                .map_or_else(|| default_json(p), |k| k.value.clone());
            (p.name.clone(), v)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use k_diagnostics::{TextRange, TextSize};

    fn param(
        name: &str,
        type_name: &str,
        control: Control,
        default: Option<ControlValue>,
        source: &str,
    ) -> PreviewParam {
        PreviewParam {
            name: name.to_owned(),
            name_range: TextRange::empty(TextSize::from(0)),
            type_name: type_name.to_owned(),
            control,
            default_source: source.to_owned(),
            default,
        }
    }

    #[test]
    fn controls_and_defaults_are_written_as_the_panel_reads_them() {
        assert_eq!(
            control_json(&Control::Choice(vec!["small".into(), "large".into()])),
            json!({"kind": "choice", "cases": ["small", "large"]})
        );
        assert_eq!(
            control_json(&Control::Parsed("k.core.Money".into())),
            json!({"kind": "parsed", "type": "k.core.Money"})
        );
        let p = param(
            "on",
            "Bool",
            Control::Toggle,
            Some(ControlValue::Bool(true)),
            "true",
        );
        assert_eq!(default_json(&p), json!(true));
        let p = param(
            "size",
            "Size",
            Control::Choice(vec!["small".into()]),
            Some(ControlValue::Case("small".into())),
            ".small",
        );
        assert_eq!(default_json(&p), json!("small"));
        let p = param("n", "Int", Control::Integer, None, "2 + 3");
        assert_eq!(
            default_json(&p),
            Value::Null,
            "a computed default isn't shown"
        );
    }

    #[test]
    fn a_parsed_control_starts_with_the_literal_or_the_parsed_string() {
        let money = Control::Parsed("k.core.Money".into());
        assert_eq!(
            default_json(&param("price", "Money", money.clone(), None, "24.00.usd")),
            json!("24.00.usd")
        );
        assert_eq!(
            default_json(&param("price", "Money", money, None, "Money.zero(.eur)")),
            Value::Null
        );
        let wait = Control::Parsed("k.core.Duration".into());
        assert_eq!(
            default_json(&param("wait", "Duration", wait, None, "5.minutes")),
            json!("5.minutes")
        );
        let date = Control::Parsed("k.core.Date".into());
        assert_eq!(
            default_json(&param(
                "day",
                "Date",
                date.clone(),
                None,
                "Date.parse(\"2026-01-01\")!"
            )),
            json!("2026-01-01")
        );
        assert_eq!(
            default_json(&param("day", "Date", date, None, "Date.parse(\"{x}\")!")),
            Value::Null,
            "an interpolation isn't plain text"
        );
    }

    #[test]
    fn values_are_checked_against_their_controls() {
        assert_eq!(decode(&Control::Toggle, &json!(true)), Ok(json!(true)));
        assert!(decode(&Control::Toggle, &json!("yes")).is_err());
        assert_eq!(decode(&Control::Integer, &json!(3.0)), Ok(json!(3)));
        assert!(decode(&Control::Integer, &json!(2.5)).is_err());
        assert!(decode(&Control::Integer, &json!(1e300)).is_err());
        assert_eq!(decode(&Control::Number, &json!(2.5)), Ok(json!(2.5)));
        assert_eq!(decode(&Control::Text, &json!("hi")), Ok(json!("hi")));
        let tone = Control::Choice(vec!["quiet".into(), "loud".into()]);
        assert_eq!(decode(&tone, &json!("loud")), Ok(json!("loud")));
        let e = decode(&tone, &json!("shouty")).expect_err("not a case");
        assert!(e.contains("quiet, loud"), "{e}");
        // A parsed control's text is checked by the type's parser, in the frame.
        assert_eq!(
            decode(&Control::Parsed("k.core.Money".into()), &json!("nonsense")),
            Ok(json!("nonsense"))
        );
        assert_eq!(
            decode(&Control::Integer, &Value::Null),
            Ok(Value::Null),
            "null is the default"
        );
    }

    #[test]
    fn a_rebuild_keeps_values_by_name_and_type() {
        let kept: BTreeMap<String, Kept> = [
            ("count", "Int", json!(7)),
            ("label", "String", json!("Cart")),
            ("tone", "Tone", json!("loud")),
            ("gone", "Bool", json!(true)),
        ]
        .into_iter()
        .map(|(n, t, v)| {
            (
                n.to_owned(),
                Kept {
                    type_name: t.to_owned(),
                    value: v,
                },
            )
        })
        .collect();
        let params = vec![
            // Same name and type: kept.
            param(
                "count",
                "Int",
                Control::Integer,
                Some(ControlValue::Int(1)),
                "1",
            ),
            // Retyped: back to the default.
            param(
                "label",
                "Int",
                Control::Integer,
                Some(ControlValue::Int(0)),
                "0",
            ),
            // The case was removed: back to the default.
            param(
                "tone",
                "Tone",
                Control::Choice(vec!["quiet".into()]),
                Some(ControlValue::Case("quiet".into())),
                ".quiet",
            ),
            // New.
            param(
                "fresh",
                "Bool",
                Control::Toggle,
                Some(ControlValue::Bool(false)),
                "false",
            ),
        ];
        let next = retain(&kept, &params);
        assert_eq!(next.keys().collect::<Vec<_>>(), vec!["count"]);
        let shown = shown(&next, &params);
        assert_eq!(shown["count"], json!(7));
        assert_eq!(shown["label"], json!(0));
        assert_eq!(shown["tone"], json!("quiet"));
        assert_eq!(shown["fresh"], json!(false));
        assert!(
            !shown.contains_key("gone"),
            "a removed parameter is forgotten"
        );
    }
}
