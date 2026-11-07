//! Postgres binary formats for the column types K exchanges as text
//! (`docs/compiler/db.md` §6, "Values"): `numeric` (decimals and money
//! amounts), `timestamptz` (dates, RFC 3339 text) and `jsonb` (`Codable`
//! values).
//!
//! `tokio-postgres` binds and reads in binary, and has no conversion
//! between those types and text without extra crates, so the driver
//! converts by hand. Every function is total: malformed input is an error,
//! never a panic.

/// An error converting a value.
pub(super) type CodecError = Box<dyn std::error::Error + Sync + Send>;

/// The sign word of a negative `numeric`.
const NUMERIC_NEG: u16 = 0x4000;
/// The sign word of `NaN`.
const NUMERIC_NAN: u16 = 0xC000;

/// Encodes decimal text (`-12.3400`, `7`, `.5`) as a binary `numeric`.
pub(super) fn encode_numeric(text: &str) -> Result<Vec<u8>, CodecError> {
    let bad = || -> CodecError { "not a decimal number".into() };
    let (negative, rest) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let (int, frac) = rest.split_once('.').unwrap_or((rest, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if (int.is_empty() && frac.is_empty()) || !digits(int) || !digits(frac) {
        return Err(bad());
    }
    let dscale = u16::try_from(frac.len()).map_err(|_| bad())?;
    let int = int.trim_start_matches('0');
    // Base-10000 groups: the integer part from the right, the fraction
    // from the left, padded with zeros.
    let mut groups: Vec<u16> = Vec::new();
    let lead = int.len() % 4;
    let mut chunks: Vec<&str> = Vec::new();
    if lead > 0 {
        chunks.push(&int[..lead]);
    }
    let mut i = lead;
    while i < int.len() {
        chunks.push(&int[i..i + 4]);
        i += 4;
    }
    for c in &chunks {
        groups.push(c.parse().map_err(|_| bad())?);
    }
    let int_groups = i32::try_from(groups.len()).map_err(|_| bad())?;
    let mut f = 0;
    while f < frac.len() {
        let end = (f + 4).min(frac.len());
        let mut chunk = frac[f..end].to_owned();
        while chunk.len() < 4 {
            chunk.push('0');
        }
        groups.push(chunk.parse().map_err(|_| bad())?);
        f += 4;
    }
    let mut weight = int_groups - 1;
    // Leading zero groups (only possible in the fraction) lower the weight.
    let leading = groups.iter().take_while(|g| **g == 0).count();
    groups.drain(..leading);
    weight -= i32::try_from(leading).map_err(|_| bad())?;
    while groups.last() == Some(&0) {
        groups.pop();
    }
    let zero = groups.is_empty();
    let weight = if zero {
        0
    } else {
        i16::try_from(weight).map_err(|_| bad())?
    };
    let ndigits = u16::try_from(groups.len()).map_err(|_| bad())?;
    let sign = if negative && !zero { NUMERIC_NEG } else { 0 };
    let mut out = Vec::with_capacity(8 + 2 * groups.len());
    out.extend_from_slice(&ndigits.to_be_bytes());
    out.extend_from_slice(&weight.to_be_bytes());
    out.extend_from_slice(&sign.to_be_bytes());
    out.extend_from_slice(&dscale.to_be_bytes());
    for g in groups {
        out.extend_from_slice(&g.to_be_bytes());
    }
    Ok(out)
}

fn be16(raw: &[u8], at: usize) -> Result<u16, CodecError> {
    raw.get(at..at + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .ok_or_else(|| "truncated numeric".into())
}

/// Decodes a binary `numeric` to decimal text with its scale (`12.3400`).
pub(super) fn decode_numeric(raw: &[u8]) -> Result<String, CodecError> {
    let ndigits = usize::from(be16(raw, 0)?);
    let weight = i32::from(i16::from_be_bytes(be16(raw, 2)?.to_be_bytes()));
    let sign = be16(raw, 4)?;
    let dscale = usize::from(be16(raw, 6)?);
    if sign == NUMERIC_NAN {
        return Err("NaN is not a decimal".into());
    }
    let mut groups = Vec::with_capacity(ndigits);
    for i in 0..ndigits {
        let g = be16(raw, 8 + 2 * i)?;
        if g > 9999 {
            return Err("malformed numeric digit".into());
        }
        groups.push(g);
    }
    let group = |exponent: i32| -> u16 {
        // Group index `weight - exponent`.
        let index = weight - exponent;
        usize::try_from(index)
            .ok()
            .and_then(|i| groups.get(i).copied())
            .unwrap_or(0)
    };
    let mut int = String::new();
    if weight >= 0 {
        for e in (0..=weight).rev() {
            if int.is_empty() {
                let g = group(e);
                if g != 0 {
                    int.push_str(&g.to_string());
                }
            } else {
                int.push_str(&format!("{:04}", group(e)));
            }
        }
    }
    if int.is_empty() {
        int.push('0');
    }
    let mut frac = String::new();
    let mut e = -1;
    while frac.len() < dscale {
        frac.push_str(&format!("{:04}", group(e)));
        e -= 1;
    }
    frac.truncate(dscale);
    let mut out = String::new();
    if sign == NUMERIC_NEG {
        out.push('-');
    }
    out.push_str(&int);
    if !frac.is_empty() {
        out.push('.');
        out.push_str(&frac);
    }
    Ok(out)
}

/// Microseconds from the Unix epoch to the Postgres epoch, 2000-01-01.
const PG_EPOCH_MICROS: i64 = 946_684_800_000_000;

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date of a day number (Hinnant's `civil_from_days`).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Parses RFC 3339 text (`2026-11-07T10:00:00.250Z`, `…+01:00`) to
/// microseconds since the Unix epoch.
pub(super) fn parse_rfc3339(text: &str) -> Result<i64, CodecError> {
    let bad = || -> CodecError { "not an RFC 3339 timestamp".into() };
    let b = text.as_bytes();
    let num = |from: usize, len: usize| -> Result<i64, CodecError> {
        let s = text.get(from..from + len).ok_or_else(bad)?;
        if !s.bytes().all(|c| c.is_ascii_digit()) {
            return Err(bad());
        }
        s.parse().map_err(|_| bad())
    };
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || !matches!(b[10], b'T' | b't' | b' ')
        || b[13] != b':'
        || b[16] != b':'
    {
        return Err(bad());
    }
    let (y, mo, d) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (h, mi, s) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return Err(bad());
    }
    let mut i = 19;
    let mut micros = 0i64;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        let digits = &text[start..i];
        if digits.is_empty() {
            return Err(bad());
        }
        let mut padded: String = digits.chars().take(6).collect();
        while padded.len() < 6 {
            padded.push('0');
        }
        micros = padded.parse().map_err(|_| bad())?;
    }
    let offset_seconds = match &text[i..] {
        "Z" | "z" => 0,
        rest if rest.len() == 6 && (rest.starts_with('+') || rest.starts_with('-')) => {
            if rest.as_bytes()[3] != b':' {
                return Err(bad());
            }
            let oh = num(i + 1, 2)?;
            let om = num(i + 4, 2)?;
            let o = oh * 3600 + om * 60;
            if rest.starts_with('-') { -o } else { o }
        }
        _ => return Err(bad()),
    };
    let days = days_from_civil(y, mo, d);
    let seconds = days * 86_400 + h * 3600 + mi * 60 + s - offset_seconds;
    seconds
        .checked_mul(1_000_000)
        .and_then(|m| m.checked_add(micros))
        .ok_or_else(bad)
}

/// Formats microseconds since the Unix epoch as RFC 3339 UTC with
/// milliseconds (`2026-11-07T10:00:00.250Z`), the text SQLite stores.
pub(super) fn format_rfc3339(micros: i64) -> String {
    let millis = micros.div_euclid(1000);
    let seconds = millis.div_euclid(1000);
    let ms = millis.rem_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{ms:03}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Encodes RFC 3339 text as a binary `timestamptz`.
pub(super) fn encode_timestamp(text: &str) -> Result<Vec<u8>, CodecError> {
    let micros = parse_rfc3339(text)?
        .checked_sub(PG_EPOCH_MICROS)
        .ok_or("timestamp out of range")?;
    Ok(micros.to_be_bytes().to_vec())
}

/// Decodes a binary `timestamptz` to RFC 3339 text.
pub(super) fn decode_timestamp(raw: &[u8]) -> Result<String, CodecError> {
    let bytes: [u8; 8] = raw.try_into().map_err(|_| "malformed timestamp")?;
    let micros = i64::from_be_bytes(bytes);
    if micros == i64::MAX || micros == i64::MIN {
        return Err("an infinite timestamp is not a date".into());
    }
    let unix = micros
        .checked_add(PG_EPOCH_MICROS)
        .ok_or("timestamp out of range")?;
    Ok(format_rfc3339(unix))
}

/// Encodes JSON text as binary `jsonb` (format version 1).
pub(super) fn encode_jsonb(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 1);
    out.push(1);
    out.extend_from_slice(text.as_bytes());
    out
}

/// Decodes binary `jsonb` to JSON text.
pub(super) fn decode_jsonb(raw: &[u8]) -> Result<String, CodecError> {
    match raw.split_first() {
        Some((1, rest)) => Ok(std::str::from_utf8(rest)?.to_owned()),
        _ => Err("unknown jsonb format version".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numeric(text: &str) -> String {
        decode_numeric(&encode_numeric(text).expect("encodes")).expect("decodes")
    }

    #[test]
    fn numerics_round_trip_with_their_scale() {
        for (input, back) in [
            ("0", "0"),
            ("7", "7"),
            ("-12.3400", "-12.3400"),
            ("10000", "10000"),
            ("123456789", "123456789"),
            ("0.0001", "0.0001"),
            ("0.00001", "0.00001"),
            ("-0.5", "-0.5"),
            ("-0", "0"),
            ("+3.10", "3.10"),
            (".5", "0.5"),
            (
                "1234567890123456789012345678.0123456789",
                "1234567890123456789012345678.0123456789",
            ),
            ("19.99", "19.99"),
        ] {
            assert_eq!(numeric(input), back, "{input}");
        }
    }

    #[test]
    fn numeric_layout_matches_postgres() {
        // 12.34: one group 12 (weight 0), one fraction group 3400, dscale 2.
        assert_eq!(
            encode_numeric("12.34").expect("encodes"),
            [0, 2, 0, 0, 0, 0, 0, 2, 0, 12, 13, 72]
        );
        // Postgres sends 0.01 as weight -1, digit 100, dscale 2.
        assert_eq!(
            decode_numeric(&[0, 1, 0xFF, 0xFF, 0, 0, 0, 2, 0, 100]).expect("decodes"),
            "0.01"
        );
    }

    #[test]
    fn malformed_numerics_are_errors() {
        for bad in ["", "-", ".", "1e5", "abc", "1.2.3", "１"] {
            assert!(encode_numeric(bad).is_err(), "{bad:?}");
        }
        assert!(decode_numeric(&[0, 1]).is_err());
        assert!(decode_numeric(&[0, 0, 0, 0, 0xC0, 0, 0, 0]).is_err());
        assert!(decode_numeric(&[0, 1, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF]).is_err());
    }

    #[test]
    fn timestamps_round_trip_in_utc_with_milliseconds() {
        for (input, back) in [
            ("2026-11-07T10:00:00Z", "2026-11-07T10:00:00.000Z"),
            ("2026-11-07T10:00:00.25Z", "2026-11-07T10:00:00.250Z"),
            ("2026-11-07T11:30:00+01:30", "2026-11-07T10:00:00.000Z"),
            ("1970-01-01T00:00:00Z", "1970-01-01T00:00:00.000Z"),
            ("1969-12-31T23:59:59.999Z", "1969-12-31T23:59:59.999Z"),
            ("2000-02-29T12:00:00Z", "2000-02-29T12:00:00.000Z"),
        ] {
            let raw = encode_timestamp(input).expect("encodes");
            assert_eq!(decode_timestamp(&raw).expect("decodes"), back, "{input}");
        }
        assert_eq!(
            encode_timestamp("2000-01-01T00:00:00Z").expect("encodes"),
            [0; 8]
        );
    }

    #[test]
    fn malformed_timestamps_are_errors() {
        for bad in [
            "",
            "2026-11-07",
            "2026-13-07T10:00:00Z",
            "2026-11-07T25:00:00Z",
            "2026-11-07T10:00:00",
            "2026-11-07T10:00:00.Z",
            "2026-11-07T10:00:00+0100",
            "20261107T10:00:00Z",
            "２026-11-07T10:00:00Z",
        ] {
            assert!(encode_timestamp(bad).is_err(), "{bad:?}");
        }
        assert!(decode_timestamp(&[1, 2, 3]).is_err());
        assert!(decode_timestamp(&i64::MAX.to_be_bytes()).is_err());
    }

    #[test]
    fn jsonb_round_trips() {
        let raw = encode_jsonb("{\"a\":1}");
        assert_eq!(raw[0], 1);
        assert_eq!(decode_jsonb(&raw).expect("decodes"), "{\"a\":1}");
        assert!(decode_jsonb(&[]).is_err());
        assert!(decode_jsonb(&[2, b'1']).is_err());
    }
}
