//! Reusable lenient-decoding primitives for wire types.
//!
//! # Open enums
//! Wire enums must never fail on a value a newer daemon invented. Mark a unit fallback variant
//! with `#[serde(other)]`:
//!
//! ```
//! # use serde::Deserialize;
//! #[derive(Deserialize, Debug, PartialEq)]
//! #[serde(rename_all = "snake_case")]
//! enum Kind { Known, #[serde(other)] Unknown }
//! assert_eq!(serde_json::from_str::<Kind>("\"future\"").unwrap(), Kind::Unknown);
//! ```

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Epoch milliseconds (protocol rule 4: all timestamps are `i64`).
pub type WireTime = i64;

/// Normalise `number | ISO-8601 string` to epoch ms. Anything else is `None`.
#[must_use]
pub fn wire_time_ms(v: &Value) -> Option<WireTime> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| {
            let f = n.as_f64()?;
            // Bounds keep the cast lossless-in-range and NaN/inf excluded.
            (f.is_finite() && f.abs() < 9.0e18).then(|| f.round() as i64)
        }),
        Value::String(s) => parse_iso_ms(s),
        _ => None,
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn num(b: &[u8], from: usize, len: usize) -> Option<i64> {
    let s = b.get(from..from.checked_add(len)?)?;
    if !s.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(s.iter().fold(0_i64, |a, d| a * 10 + i64::from(d - b'0')))
}

/// `YYYY-MM-DDTHH:MM:SS[.fff](Z|±hh:mm)` to epoch ms.
fn parse_iso_ms(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let (y, mo, d) = (num(b, 0, 4)?, num(b, 5, 2)?, num(b, 8, 2)?);
    let (h, mi, sec) = (num(b, 11, 2)?, num(b, 14, 2)?, num(b, 17, 2)?);
    if b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') || b.get(13) != Some(&b':') || b.get(16) != Some(&b':') {
        return None;
    }
    if !matches!(b.get(10), Some(b'T' | b't' | b' ')) {
        return None;
    }
    let dim = match mo {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        _ => return None,
    };
    if d < 1 || d > dim || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    let mut i = 19;
    let mut ms = 0;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return None;
        }
        let mut scale = 100;
        for dg in &b[start..i] {
            ms += i64::from(dg - b'0') * scale;
            scale /= 10;
        }
    }
    let offset_min = match b.get(i)? {
        b'Z' | b'z' => {
            i += 1;
            0
        }
        sign @ (b'+' | b'-') => {
            let (oh, om) = (num(b, i + 1, 2)?, num(b, i + 4, 2)?);
            if b.get(i + 3) != Some(&b':') || oh > 23 || om > 59 {
                return None;
            }
            i += 6;
            let m = oh * 60 + om;
            if *sign == b'-' { -m } else { m }
        }
        _ => return None,
    };
    if i != b.len() {
        return None;
    }
    let secs = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + sec - offset_min * 60;
    Some(secs * 1000 + ms)
}

/// `deserialize_with` helper: a wrong-typed optional field becomes `None` instead of failing
/// the whole message. Pair with `#[serde(default)]` so a missing field is also `None`.
///
/// # Errors
/// Only if the underlying deserializer itself fails (not on a type mismatch).
pub fn lenient_opt<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let v = Value::deserialize(d)?;
    Ok(T::deserialize(v).ok())
}

/// Truncate `s` to at most `max_bytes` on a UTF-8 boundary; the flag reports truncation.
#[must_use]
pub fn cap_str(s: &str, max_bytes: usize) -> (String, bool) {
    if s.len() <= max_bytes {
        return (s.to_owned(), false);
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    (s[..end].to_owned(), true)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[test]
    fn wire_time_table() {
        let cases: Vec<(Value, Option<i64>)> = vec![
            (json!(1_700_000_000_000_i64), Some(1_700_000_000_000)),
            (json!("2026-11-13T22:13:10Z"), Some(1_794_607_990_000)),
            (json!("2026-11-13T22:13:10.250+01:00"), Some(1_794_604_390_250)),
            (json!("2026-11-13T22:13:10.5-02:00"), Some(1_794_615_190_500)),
            (json!("1970-01-01T00:00:00Z"), Some(0)),
            (json!("2024-02-29T00:00:00Z"), Some(1_709_164_800_000)),
            (json!("2023-02-29T00:00:00Z"), None),
            (json!("garbage"), None),
            (json!(""), None),
            (json!("2026-11-13T22:13:10"), None),
            (json!(null), None),
            (json!(true), None),
            (json!(1.5e3), Some(1500)),
        ];
        for (v, want) in cases {
            assert_eq!(wire_time_ms(&v), want, "{v}");
        }
    }

    #[test]
    fn wire_time_non_ascii_does_not_panic() {
        assert_eq!(wire_time_ms(&json!("2026-11-13T22:13:1é")), None);
        assert_eq!(wire_time_ms(&json!("é")), None);
    }

    #[derive(Deserialize, Debug)]
    struct Msg {
        #[serde(default, deserialize_with = "lenient_opt")]
        count: Option<u32>,
        name: String,
    }

    #[test]
    fn lenient_opt_wrong_type_is_none() {
        let m: Msg = serde_json::from_str(r#"{"count":"x","name":"n"}"#).unwrap();
        assert_eq!((m.count, m.name.as_str()), (None, "n"));
        let m: Msg = serde_json::from_str(r#"{"count":3,"name":"n"}"#).unwrap();
        assert_eq!(m.count, Some(3));
        let m: Msg = serde_json::from_str(r#"{"name":"n"}"#).unwrap();
        assert_eq!(m.count, None);
    }

    #[test]
    fn cap_str_boundaries() {
        assert_eq!(cap_str("abc", 3), ("abc".into(), false));
        assert_eq!(cap_str("abcd", 3), ("abc".into(), true));
        let (s, t) = cap_str("aé", 2); // é is 2 bytes at 1..3
        assert_eq!((s.as_str(), t), ("a", true));
        assert_eq!(cap_str("日本", 0), (String::new(), true));
    }

    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "snake_case")]
    enum Sample {
        Idle,
        Busy,
        #[serde(other)]
        Unknown,
    }

    #[test]
    fn open_enum_falls_back() {
        let v: Vec<Sample> = serde_json::from_str(r#"["idle","busy","from_the_future"]"#).unwrap();
        assert_eq!(v, [Sample::Idle, Sample::Busy, Sample::Unknown]);
    }
}
