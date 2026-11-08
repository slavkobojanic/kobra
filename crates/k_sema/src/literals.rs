//! The formats of the standard library's checked literals
//! (`docs/design/20-standard-library.md`, "Literals and parsing").
//!
//! `Url("…")`, `Uuid("…")`, `EmailAddress("…")`, `TimeOfDay("…")`,
//! `HttpStatus(code:)` and money amounts such as `24.00.usd` are validated
//! when the program is compiled (K2404, sema.md §9.7), and code generation
//! emits the normalized value as a constant. Text known only at run time
//! goes through the runtime's `Type.parse`, so the two must agree exactly:
//! each function here is a port of the runtime's parser
//! (`crates/k_runtime/js/runtime/std.js`: `parseEmail`, `parseUrl`,
//! `parseUuid`; `money.js`: the currency table), written as a scanner over
//! the regular expressions those parsers use, with JavaScript's definitions
//! of whitespace (`\s`, `trim`) and of the line terminators `.` refuses. A
//! differential test runs both on the same inputs
//! (`crates/k_server/tests/literals.rs`).
//!
//! Each validating function returns the normalized value or the reason the
//! text is refused, phrased to follow "not an email address: " and the like
//! in a diagnostic.
//!
//! ```
//! use k_sema::literals::{email_address, url, uuid};
//!
//! assert_eq!(email_address("  Ana@Example.COM ").as_deref(), Ok("Ana@example.com"));
//! assert_eq!(email_address("nobody"), Err("it has no `@`".to_owned()));
//! assert!(url("javascript://alert(1)").is_err());
//! assert_eq!(uuid("0B6F2C1E-4D1A-4F8E-9A3B-2C5D6E7F8A9B").as_deref(), Ok("0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9b"));
//! ```

/// The currencies with number extensions (`24.00.usd`), by extension name,
/// with their minor-unit digits, in the order of the stdlib's `Currency`
/// cases and of the runtime's `CURRENCY_DIGITS` (`money.js`).
pub const CURRENCIES: &[(&str, u32)] =
    &[("usd", 2), ("eur", 2), ("gbp", 2), ("jpy", 0), ("chf", 2)];

/// The schemes a `Url` may have: never `javascript:`, `data:` or another
/// script scheme (`URL_SCHEMES` in `std.js`).
const URL_SCHEMES: &[&str] = &["http", "https", "ws", "wss"];

/// `true` for the characters JavaScript's `\s` matches, which are also
/// what `String.prototype.trim` removes: the ECMAScript `WhiteSpace` and
/// `LineTerminator` code points.
///
/// Rust's [`char::is_whitespace`] differs (it includes U+0085 and omits
/// U+FEFF), so the ports use this instead.
#[must_use]
pub fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\u{9}'..='\u{d}'
            | ' '
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

/// `true` for the line terminators a JavaScript `.` doesn't match.
fn is_js_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `text` with leading and trailing JavaScript whitespace removed, as
/// `String.prototype.trim` does.
fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

// ------------------------------------------------------------------ email

/// `true` for the control characters an address never contains
/// (`CONTROLS` in `std.js`).
fn is_email_control(c: char) -> bool {
    matches!(c, '\u{0}'..='\u{1f}' | '\u{7f}')
}

/// `true` for the characters `EMAIL_RE` refuses in both parts of a bare
/// address: whitespace, `@`, brackets, `\`, `,`, `;`, `:` and `"`.
fn is_email_excluded(c: char) -> bool {
    is_js_space(c)
        || matches!(
            c,
            '@' | '<' | '>' | '(' | ')' | '[' | ']' | '\\' | ',' | ';' | ':' | '"'
        )
}

/// `true` for RFC 5322's specials less `.` (`SPECIALS` in `std.js`): the
/// characters a display name needs quotes for.
fn is_name_special(c: char) -> bool {
    matches!(
        c,
        '(' | ')' | '<' | '>' | '[' | ']' | ':' | ';' | '@' | '\\' | ',' | '"'
    )
}

/// `bareEmail`: a bare address with its domain lower-cased, or `None`.
///
/// `EMAIL_RE` is `^X+@X+\.Y+$`, where `X` excludes the characters of
/// [`is_email_excluded`] and `Y` also excludes `.`; so there is exactly one
/// `@`, and the domain has a `.` with something before it and only
/// non-dots after the last one.
fn bare_email(t: &str) -> Option<String> {
    if t.chars().any(|c| is_email_excluded(c) && c != '@') {
        return None;
    }
    let (local, domain) = t.split_once('@')?;
    if local.is_empty() || domain.contains('@') {
        return None;
    }
    let dot = domain.rfind('.')?;
    if dot == 0 || dot + 1 == domain.len() {
        return None;
    }
    Some(format!("{local}@{}", domain.to_lowercase()))
}

/// `NAMED_EMAIL` (`^(.*?)\s*<([^<>]*)>$`): the display name as written
/// (without the whitespace before `<`) and the text in angle brackets, or
/// `None` when `t` isn't of that form.
fn named_email(t: &str) -> Option<(&str, &str)> {
    let body = t.strip_suffix('>')?;
    let lt = body.rfind('<')?;
    let address = &body[lt + 1..];
    if address.contains('>') {
        return None;
    }
    let name = body[..lt].trim_end_matches(is_js_space);
    if name.chars().any(is_js_line_terminator) {
        return None;
    }
    Some((name, address))
}

/// The inside of a quoted display name (`QUOTED_NAME`,
/// `^"((?:[^"\\]|\\.)*)"$`) with its escapes resolved (`ESCAPE`), or
/// `None` when `t` isn't a quoted name.
fn quoted_name(t: &str) -> Option<String> {
    let inner = t.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return None,
            '\\' => match chars.next() {
                Some(e) if !is_js_line_terminator(e) => out.push(e),
                _ => return None,
            },
            _ => out.push(c),
        }
    }
    Some(out)
}

/// `nameOf`: a display name as written, quoted or plain words without
/// specials, unquoted and with its whitespace runs collapsed to one space;
/// `None` if it isn't one or is empty.
fn display_name(t: &str) -> Option<String> {
    let raw = match quoted_name(t) {
        Some(unquoted) => unquoted,
        None if t.chars().any(is_name_special) => String::new(),
        None => t.to_owned(),
    };
    let mut collapsed = String::with_capacity(raw.len());
    let mut in_space = false;
    for c in raw.chars() {
        if is_js_space(c) {
            if !in_space {
                collapsed.push(' ');
            }
            in_space = true;
        } else {
            collapsed.push(c);
            in_space = false;
        }
    }
    let name = js_trim(&collapsed);
    (!name.is_empty()).then(|| name.to_owned())
}

/// `EmailAddress.parse` (`docs/design/31-email.md`): a bare address, or a
/// display name and an address in angle brackets (`K Shop
/// <orders@shop.example>`), in its normalized message-header form. The
/// text is trimmed and the domain lower-cased; the name is unquoted unless
/// it holds a special, then quoted with `"` and `\` escaped.
///
/// # Returns
///
/// The normalized address, or `None` if `text` isn't an address or
/// contains a line break or another control character, exactly when the
/// runtime's `parseEmail` returns `null`.
#[must_use]
pub fn parse_email(text: &str) -> Option<String> {
    if text.chars().any(is_email_control) {
        return None;
    }
    let t = js_trim(text);
    let Some((name, address)) = named_email(t) else {
        return bare_email(t);
    };
    let address = bare_email(address)?;
    let name = display_name(name)?;
    if name.chars().any(is_name_special) {
        let mut quoted = String::with_capacity(name.len() + 2);
        quoted.push('"');
        for c in name.chars() {
            if matches!(c, '"' | '\\') {
                quoted.push('\\');
            }
            quoted.push(c);
        }
        quoted.push('"');
        Some(format!("{quoted} <{address}>"))
    } else {
        Some(format!("{name} <{address}>"))
    }
}

/// Validates an `EmailAddress("…")` literal.
///
/// # Errors
///
/// Why [`parse_email`] refuses `text`, for "not an email address: …".
pub fn email_address(text: &str) -> Result<String, String> {
    parse_email(text).ok_or_else(|| email_problem(text))
}

/// Why [`parse_email`] refuses `text` (which it does).
fn email_problem(text: &str) -> String {
    if text.chars().any(is_email_control) {
        return "it contains a line break or another control character".to_owned();
    }
    let t = js_trim(text);
    if t.is_empty() {
        return "it is empty".to_owned();
    }
    if let Some((name, address)) = named_email(t) {
        if bare_email(address).is_some() {
            return if name.is_empty() {
                "the display name before `<` is empty; write the address alone".to_owned()
            } else {
                "a display name with `,`, `@` or another special character is quoted, as in `\"Doe, Ana\" <ana@example.com>`"
                    .to_owned()
            };
        }
        return format!("the address in `<…>` isn't one: {}", bare_problem(address));
    }
    bare_problem(t)
}

/// Why [`bare_email`] refuses `t` (which it does).
fn bare_problem(t: &str) -> String {
    if t.is_empty() {
        return "it is empty".to_owned();
    }
    if t.contains(['<', '>']) {
        return "`<` and `>` enclose the address after a display name, as in `Ana <ana@example.com>`"
            .to_owned();
    }
    if let Some(c) = t.chars().find(|c| matches!(c, ',' | ';')) {
        return format!(
            "it holds a `{c}`, which separates addresses, and an `EmailAddress` is one address"
        );
    }
    if t.chars().any(is_js_space) {
        return "it contains a space".to_owned();
    }
    if let Some(c) = t.chars().find(|&c| is_email_excluded(c) && c != '@') {
        return format!("`{c}` can't appear in an address outside a quoted display name");
    }
    let ats = t.matches('@').count();
    if ats == 0 {
        return "it has no `@`".to_owned();
    }
    if ats > 1 {
        return "it has more than one `@`".to_owned();
    }
    let (local, domain) = t.split_once('@').unwrap_or((t, ""));
    if local.is_empty() {
        return "nothing comes before the `@`".to_owned();
    }
    if domain.is_empty() {
        return "nothing comes after the `@`".to_owned();
    }
    match domain.rfind('.') {
        None => format!("the domain `{domain}` has no `.`"),
        Some(0) => format!("the domain `{domain}` has nothing before its last `.`"),
        Some(_) => format!("the domain `{domain}` ends with `.`"),
    }
}

// ------------------------------------------------------------------ url

/// Where [`url_parts`] stopped, for the reason in [`url`].
enum UrlProblem {
    /// A space, a control character or `\` (`URL_BAD`).
    Bad,
    /// No `scheme://`.
    NoScheme,
    /// Nothing that is a host after `://` (and any user info).
    NoHost,
    /// A `[` host that isn't hexadecimal digits, `:` and `.` before `]`.
    BadIpv6,
    /// A port that isn't 1 to 5 digits.
    BadPort,
    /// Something other than a port, a path, a query or a fragment after
    /// the host.
    AfterHost,
}

/// The scheme of `text` if it matches `URL_RE`:
/// `^([a-zA-Z][a-zA-Z0-9+.-]*)://(?:[^/?#@]*@)?(\[[0-9a-fA-F:.]+\]|[^/?#:@\[\]]+)(?::[0-9]{1,5})?([/?#].*)?$`,
/// after `URL_BAD` (`[\u0000- \u007f\\]`) found nothing.
///
/// Every part of the expression has one way to match, so a single scan
/// decides it: the scheme ends at the first `:`, user info runs to an `@`
/// that comes before any `/`, `?` or `#`, and the host and port are
/// maximal.
fn url_parts(text: &str) -> Result<&str, UrlProblem> {
    if text
        .chars()
        .any(|c| matches!(c, '\u{0}'..='\u{20}' | '\u{7f}' | '\\'))
    {
        return Err(UrlProblem::Bad);
    }
    let (scheme, rest) = text.split_once("://").ok_or(UrlProblem::NoScheme)?;
    let mut sc = scheme.chars();
    if !sc.next().is_some_and(|c| c.is_ascii_alphabetic())
        || !sc.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
    {
        return Err(UrlProblem::NoScheme);
    }
    let rest = match rest.find(['/', '?', '#', '@']) {
        Some(i) if rest[i..].starts_with('@') => &rest[i + 1..],
        _ => rest,
    };
    let after_host = if let Some(inner) = rest.strip_prefix('[') {
        let close = inner.find(']').ok_or(UrlProblem::BadIpv6)?;
        let address = &inner[..close];
        if address.is_empty()
            || !address
                .chars()
                .all(|c| c.is_ascii_hexdigit() || matches!(c, ':' | '.'))
        {
            return Err(UrlProblem::BadIpv6);
        }
        &inner[close + 1..]
    } else {
        let end = rest
            .find(['/', '?', '#', ':', '@', '[', ']'])
            .unwrap_or(rest.len());
        if end == 0 {
            return Err(UrlProblem::NoHost);
        }
        &rest[end..]
    };
    let after_port = match after_host.strip_prefix(':') {
        Some(port) => {
            let digits = port
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(port.len());
            if digits == 0 || digits > 5 {
                return Err(UrlProblem::BadPort);
            }
            let tail = &port[digits..];
            if !tail.is_empty() && !tail.starts_with(['/', '?', '#']) {
                return Err(UrlProblem::BadPort);
            }
            tail
        }
        None => after_host,
    };
    if !after_port.is_empty()
        && (!after_port.starts_with(['/', '?', '#'])
            || after_port.chars().any(is_js_line_terminator))
    {
        return Err(UrlProblem::AfterHost);
    }
    Ok(scheme)
}

/// `Url.parse`: `true` for an absolute `http`, `https`, `ws` or `wss` URL
/// with a host, exactly when the runtime's `parseUrl` returns the text.
#[must_use]
pub fn parse_url(text: &str) -> bool {
    url_parts(text).is_ok_and(|scheme| URL_SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()))
}

/// Validates a `Url("…")` literal. A `Url` is its text, unchanged.
///
/// # Errors
///
/// Why [`parse_url`] refuses `text`, for "not a URL: …".
pub fn url(text: &str) -> Result<String, String> {
    let scheme = url_parts(text).map_err(|p| {
        match p {
            UrlProblem::Bad => "it contains a space, a control character or `\\`",
            UrlProblem::NoScheme => "it needs a scheme and `://`, as in `https://example.com`",
            UrlProblem::NoHost => "it has no host after `://`",
            UrlProblem::BadIpv6 => "its host in `[…]` isn't an IPv6 address",
            UrlProblem::BadPort => "a port is 1 to 5 digits",
            UrlProblem::AfterHost => {
                "its host is followed by something other than a port, a path, a query or a fragment"
            }
        }
        .to_owned()
    })?;
    if URL_SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()) {
        Ok(text.to_owned())
    } else {
        Err(format!(
            "a `Url` is `http`, `https`, `ws` or `wss`, never `{scheme}`"
        ))
    }
}

// ------------------------------------------------------------------ uuid

/// `Uuid.parse`: canonical `8-4-4-4-12` hexadecimal text in either case,
/// as lower case; `None` otherwise, exactly when the runtime's `parseUuid`
/// returns `null`.
#[must_use]
pub fn parse_uuid(text: &str) -> Option<String> {
    let groups: Vec<&str> = text.split('-').collect();
    let shaped = groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, n)| g.len() == n && g.bytes().all(|b| b.is_ascii_hexdigit()));
    shaped.then(|| text.to_ascii_lowercase())
}

/// Validates a `Uuid("…")` literal.
///
/// # Errors
///
/// Why [`parse_uuid`] refuses `text`, for "not a UUID: …".
pub fn uuid(text: &str) -> Result<String, String> {
    parse_uuid(text).ok_or_else(|| {
        "a UUID is 32 hexadecimal digits in groups of 8-4-4-4-12, such as `0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9b`"
            .to_owned()
    })
}

// ------------------------------------------------------------------ time of day

/// Validates a `TimeOfDay("…")` literal: `HH:MM` on a 24-hour clock.
///
/// # Returns
///
/// The hour (`0...23`) and the minute (`0...59`).
///
/// # Errors
///
/// Why `text` isn't one, for "not a time of day: …".
pub fn time_of_day(text: &str) -> Result<(u32, u32), String> {
    let form = || "write it as `HH:MM` on a 24-hour clock, such as `03:00`".to_owned();
    let (h, m) = text.split_once(':').ok_or_else(form)?;
    let two = |s: &str| {
        (s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<u32>().ok())
            .flatten()
    };
    let (Some(hour), Some(minute)) = (two(h), two(m)) else {
        return Err(form());
    };
    if hour > 23 {
        return Err("the hour is `00` to `23`".to_owned());
    }
    if minute > 59 {
        return Err("the minute is `00` to `59`".to_owned());
    }
    Ok((hour, minute))
}

// ------------------------------------------------------------------ http status

/// Validates the code of an `HttpStatus(code:)` literal: `100...599`.
///
/// # Errors
///
/// Why `code` isn't a status, for "not an HTTP status: …".
pub fn http_status_code(code: i128) -> Result<u16, String> {
    u16::try_from(code)
        .ok()
        .filter(|c| (100..=599).contains(c))
        .ok_or_else(|| "a status code is `100` to `599`".to_owned())
}

// ------------------------------------------------------------------ money

/// The minor-unit digits of the currency whose number extension is `name`
/// (`usd` → 2, `jpy` → 0), or `None` for a name that isn't one.
#[must_use]
pub fn currency_digits(name: &str) -> Option<u32> {
    CURRENCIES.iter().find(|(n, _)| *n == name).map(|(_, d)| *d)
}

/// The decimal places a number literal's value has, ignoring trailing
/// zeros as the runtime's `money` does: `1.005` → 3, `1.500` → 1,
/// `1500` → 0, `1.5e-2` → 3.
///
/// # Returns
///
/// `None` for text that isn't a decimal number literal; `0` for
/// hexadecimal, octal and binary integers.
#[must_use]
pub fn decimal_places(literal: &str) -> Option<u64> {
    let text: String = literal.chars().filter(|c| *c != '_').collect();
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("0x") || lower.starts_with("0o") || lower.starts_with("0b") {
        return Some(0);
    }
    let (mantissa, exponent) = match lower.split_once('e') {
        Some((m, e)) => (m, e.parse::<i64>().ok()?),
        None => (lower.as_str(), 0),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()) || int.len() + frac.len() == 0 {
        return None;
    }
    let digits = format!("{int}{frac}");
    let mut scale = i64::try_from(frac.len()).ok()?.checked_sub(exponent)?;
    let mut trailing = digits.bytes().rev().take_while(|b| *b == b'0');
    while scale > 0 && trailing.next().is_some() {
        scale -= 1;
    }
    Some(u64::try_from(scale.max(0)).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_addresses_are_trimmed_and_their_domain_lower_cased() {
        assert_eq!(
            parse_email("  Ana@Example.COM ").as_deref(),
            Some("Ana@example.com")
        );
        for bad in [
            "nobody",
            "<script>alert(1)</script>",
            "a@b",
            "a b@c.de",
            "a@b.c\nBcc: x@y.z",
            "<a@b.cd>",
            "a@@b.cd",
            "",
            "a@b.cd,c@d.ef",
            "a@.cd",
            "a@b.",
            "@b.cd",
        ] {
            assert_eq!(parse_email(bad), None, "{bad:?}");
        }
        // `X+\.Y+` lets dots precede the last one.
        assert_eq!(parse_email("a@..cd").as_deref(), Some("a@..cd"));
    }

    #[test]
    fn display_names_are_unquoted_unless_they_need_quotes() {
        assert_eq!(
            parse_email("  K Shop   <orders@Shop.EXAMPLE> ").as_deref(),
            Some("K Shop <orders@shop.example>")
        );
        assert_eq!(
            parse_email("\"Ana\" <ana@example.com>").as_deref(),
            Some("Ana <ana@example.com>")
        );
        assert_eq!(
            parse_email("J. Doe <j@doe.example>").as_deref(),
            Some("J. Doe <j@doe.example>")
        );
        assert_eq!(
            parse_email("\"Doe, Ana\" <ana@example.com>").as_deref(),
            Some("\"Doe, Ana\" <ana@example.com>")
        );
        assert_eq!(
            parse_email("\"Ana \\\"the\\\" Doe\" <ana@example.com>").as_deref(),
            Some("\"Ana \\\"the\\\" Doe\" <ana@example.com>")
        );
        for bad in [
            "Doe, Ana <ana@example.com>",
            "Ana <ana@example.com",
            "Ana <nobody>",
            "Ana\nBcc: x@y.z <a@b.cd>",
            "Ana <a@b.cd> x",
            "\"Ana <a@b.cd>",
            "<a@b.cd>",
            "Ana <a@b.cd>, Bo <b@c.de>",
            "A<n>a <a@b.cd>",
            "\"  \" <a@b.cd>",
            "Ana\u{2028}Doe <a@b.cd>",
        ] {
            assert_eq!(parse_email(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn javascript_whitespace_is_not_rusts() {
        // U+FEFF is JavaScript whitespace; U+0085 isn't.
        assert_eq!(
            parse_email("\u{feff}a@b.cd\u{feff}").as_deref(),
            Some("a@b.cd")
        );
        assert!(parse_email("\u{85}a@b.cd").is_some_and(|a| a.starts_with('\u{85}')));
    }

    #[test]
    fn every_refusal_has_a_reason() {
        assert_eq!(
            email_address("ana@localhost"),
            Err("the domain `localhost` has no `.`".to_owned())
        );
        assert_eq!(
            email_address("Ana <nobody>"),
            Err("the address in `<…>` isn't one: it has no `@`".to_owned())
        );
        assert_eq!(
            email_address("a(b@c.de"),
            Err("`(` can't appear in an address outside a quoted display name".to_owned())
        );
        assert_eq!(
            email_address("a@b."),
            Err("the domain `b.` ends with `.`".to_owned())
        );
        assert_eq!(
            email_address("a@.b"),
            Err("the domain `.b` has nothing before its last `.`".to_owned())
        );
    }

    #[test]
    fn urls_are_absolute_with_a_safe_scheme() {
        for good in [
            "https://api.stripe.com",
            "http://shop.internal:8080",
            "wss://k.dev/live?room=1#top",
            "https://user:pw@[::1]:8443/p",
            "HTTPS://K.DEV",
            "https://k.dev?q",
            "https://k.dev#f",
            "https://a@b/c@d",
        ] {
            assert!(parse_url(good), "{good:?}");
            assert_eq!(url(good).as_deref(), Ok(good));
        }
        for bad in [
            "/icon.svg",
            "javascript://alert(1)",
            "javascript:alert(1)",
            "data://x",
            "https://",
            "https://exa mple.com",
            "https://k.dev:123456",
            "https://k.dev:",
            "https://k.dev:80x",
            "https://[zz]/",
            "https://[]/",
            "https://a@b@c",
            "https://k.dev\\x",
            "https://k.dev/\u{2028}",
            "1https://k.dev",
            "https://:80",
        ] {
            assert!(!parse_url(bad), "{bad:?}");
            assert!(url(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn uuids_are_lower_cased() {
        assert_eq!(
            parse_uuid("0B6F2C1E-4D1A-4F8E-9A3B-2C5D6E7F8A9B").as_deref(),
            Some("0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9b")
        );
        for bad in [
            "0b6f2c1e",
            "0b6f2c1e4d1a4f8e9a3b2c5d6e7f8a9b",
            "{0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9b}",
            "0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9g",
            "0b6f2c1e-4d1a-4f8e-9a3b-2c5d6e7f8a9b-",
        ] {
            assert_eq!(parse_uuid(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn times_of_day_are_hh_mm() {
        assert_eq!(time_of_day("03:00"), Ok((3, 0)));
        assert_eq!(time_of_day("23:59"), Ok((23, 59)));
        assert!(time_of_day("24:00").is_err());
        assert!(time_of_day("12:60").is_err());
        assert!(time_of_day("3:00").is_err());
        assert!(time_of_day("+3:00").is_err());
        assert!(time_of_day("").is_err());
    }

    #[test]
    fn status_codes_are_100_to_599() {
        assert_eq!(http_status_code(418), Ok(418));
        assert!(http_status_code(99).is_err());
        assert!(http_status_code(600).is_err());
        assert!(http_status_code(-200).is_err());
        assert!(http_status_code(i128::MAX).is_err());
    }

    #[test]
    fn decimal_places_ignore_trailing_zeros() {
        assert_eq!(decimal_places("1.005"), Some(3));
        assert_eq!(decimal_places("1.500"), Some(1));
        assert_eq!(decimal_places("24.00"), Some(0));
        assert_eq!(decimal_places("1_500"), Some(0));
        assert_eq!(decimal_places("1.5e-2"), Some(3));
        assert_eq!(decimal_places("15e1"), Some(0));
        assert_eq!(decimal_places("0x10"), Some(0));
        assert_eq!(decimal_places("1e99999999999999999999"), None);
        assert_eq!(currency_digits("jpy"), Some(0));
        assert_eq!(currency_digits("usd"), Some(2));
        assert_eq!(currency_digits("xyz"), None);
    }

    #[test]
    fn the_currency_table_is_the_runtimes() {
        let money = include_str!("../../k_runtime/js/runtime/money.js");
        let digits: Vec<String> = CURRENCIES.iter().map(|(_, d)| d.to_string()).collect();
        let codes: Vec<String> = CURRENCIES
            .iter()
            .map(|(n, _)| format!("\"{}\"", n.to_ascii_uppercase()))
            .collect();
        assert!(money.contains(&format!("const CURRENCY_DIGITS = [{}];", digits.join(", "))));
        assert!(money.contains(&format!("const CURRENCY_CODES = [{}];", codes.join(", "))));
    }
}
