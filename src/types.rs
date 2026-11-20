//! Value types shared by every DATEX II version.

use std::fmt;

/// A calendar date, with the time-zone offset if the document gave one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Date {
    /// Year, may be negative.
    pub year: i32,
    /// Month, 1 to 12.
    pub month: u8,
    /// Day of the month, 1 to 31.
    pub day: u8,
}

/// A time of day with nanosecond precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Time {
    /// Hour, 0 to 24.
    pub hour: u8,
    /// Minute, 0 to 59.
    pub minute: u8,
    /// Second, 0 to 60.
    pub second: u8,
    /// Fraction of the second in nanoseconds.
    pub nanosecond: u32,
    /// How many digits of the fraction the document wrote, 0 to 9. A value read keeps them, so
    /// that `12:00:00.010` is written back as `12:00:00.010`. When the nanoseconds need more
    /// digits than this, they are written anyway.
    pub fraction_digits: u8,
}

/// A point in time as written in the document.
///
/// Nothing is converted: the offset is kept as given, and a timestamp without a time zone keeps
/// `offset_minutes == None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DateTime {
    /// The date part.
    pub date: Date,
    /// The time part.
    pub time: Time,
    /// Offset from UTC in minutes; `Some(0)` for `Z`, `None` when the document gave no time zone.
    pub offset_minutes: Option<i16>,
}

impl DateTime {
    /// Seconds since 1970-01-01T00:00:00Z, or `None` when the timestamp has no time zone.
    pub fn unix_timestamp(&self) -> Option<i64> {
        let offset = i64::from(self.offset_minutes?);
        let d = self.date;
        // Days from civil (Howard Hinnant's algorithm).
        let y = i64::from(d.year) - i64::from(d.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y.rem_euclid(400);
        let m = i64::from(d.month);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        let t = self.time;
        Some(
            days * 86_400
                + i64::from(t.hour) * 3600
                + i64::from(t.minute) * 60
                + i64::from(t.second)
                - offset * 60,
        )
    }
}

fn write_offset(f: &mut fmt::Formatter<'_>, offset: Option<i16>) -> fmt::Result {
    match offset {
        None => Ok(()),
        Some(0) => f.write_str("Z"),
        Some(m) => {
            let sign = if m < 0 { '-' } else { '+' };
            let m = m.unsigned_abs();
            write!(f, "{sign}{:02}:{:02}", m / 60, m % 60)
        }
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}:{:02}", self.hour, self.minute, self.second)?;
        let all = format!("{:09}", self.nanosecond.min(999_999_999));
        let needed = all.trim_end_matches('0').len();
        let shown = needed.max(usize::from(self.fraction_digits.min(9)));
        if shown > 0 {
            write!(f, ".{}", &all[..shown])?;
        }
        Ok(())
    }
}

impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}T{}", self.date, self.time)?;
        write_offset(f, self.offset_minutes)
    }
}

/// A decimal number kept exactly as written, so that prices never pass through a float.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Decimal(String);

impl Decimal {
    /// The number as written in the document.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The nearest `f64`.
    pub fn to_f64(&self) -> f64 {
        self.0.parse().unwrap_or(f64::NAN)
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A text in one language.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LangString {
    /// The language code as written (usually ISO 639-1, but feeds vary).
    pub lang: String,
    /// The text.
    pub text: String,
}

/// A text given in zero or more languages.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct MultilingualString {
    /// One entry per language, in document order.
    pub values: Vec<LangString>,
}

impl MultilingualString {
    /// A text in one language.
    ///
    /// ```
    /// let name = datex2::MultilingualString::new("de", "Ladepark Mitte");
    /// assert_eq!(name.get("de"), Some("Ladepark Mitte"));
    /// ```
    ///
    /// For more languages, push to [`values`](Self::values).
    pub fn new(lang: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            values: vec![LangString {
                lang: lang.into(),
                text: text.into(),
            }],
        }
    }

    /// The text in the given language, compared without regard to case.
    pub fn get(&self, lang: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|v| v.lang.eq_ignore_ascii_case(lang))
            .map(|v| v.text.as_str())
    }

    /// The first text, whatever its language.
    pub fn first(&self) -> Option<&str> {
        self.values.first().map(|v| v.text.as_str())
    }
}

/// Content of an extension slot, kept as it was written.
///
/// DATEX II lets every class carry extension content defined outside the standard. The reader does
/// not interpret it; it keeps it in the encoding it came in, so that it can be parsed by whoever
/// knows the extension and written back unchanged. Content read from XML cannot be written as JSON
/// and the reverse; the writer says so.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Extension {
    /// XML: the content between the start and end tag of the extension element.
    pub xml: String,
    /// XML: prefix and namespace URI of the bindings in scope that the content uses; the default
    /// namespace has an empty prefix.
    pub namespaces: Vec<(String, String)>,
    /// JSON: the extension object as written, braces included.
    pub json: String,
}

impl Extension {
    /// The namespace declarations the content needs, as attribute text (` xmlns:p="uri"` …),
    /// leaving out those for which `already` holds.
    pub(crate) fn declarations(&self, already: impl Fn(&str) -> bool) -> String {
        use fmt::Write as _;
        let mut out = String::new();
        for (prefix, uri) in &self.namespaces {
            let name = if prefix.is_empty() {
                "xmlns".to_owned()
            } else {
                format!("xmlns:{prefix}")
            };
            let safe = prefix
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
                && !uri.contains(['"', '<', '&']);
            if safe && !already(&name) {
                let _ = write!(out, " {name}=\"{uri}\"");
            }
        }
        out
    }
}

/// A value that can be read from the text of an element or attribute.
pub(crate) trait FromText: Sized {
    /// Words for "what was expected" in an error.
    const EXPECTED: &'static str;
    /// Parses trimmed text.
    fn from_text(text: &str) -> Option<Self>;
}

macro_rules! from_text_parse {
    ($($t:ty => $e:literal),*) => {$(
        impl FromText for $t {
            const EXPECTED: &'static str = $e;
            fn from_text(text: &str) -> Option<Self> {
                text.strip_prefix('+').unwrap_or(text).parse().ok()
            }
        }
    )*};
}
from_text_parse!(i32 => "an integer", i64 => "an integer", u64 => "a non-negative integer");

macro_rules! from_text_float {
    ($($t:ty),*) => {$(
        impl FromText for $t {
            const EXPECTED: &'static str = "a number";
            fn from_text(text: &str) -> Option<Self> {
                match text {
                    "INF" | "+INF" => Some(<$t>::INFINITY),
                    "-INF" => Some(<$t>::NEG_INFINITY),
                    "NaN" => Some(<$t>::NAN),
                    // Rust accepts spellings XML Schema does not ("inf", "nan", "infinity").
                    _ if text.bytes().any(|b| b.is_ascii_alphabetic() && b != b'e' && b != b'E') => None,
                    _ => text.parse().ok(),
                }
            }
        }
    )*};
}
from_text_float!(f32, f64);

impl FromText for bool {
    const EXPECTED: &'static str = "true or false";
    fn from_text(text: &str) -> Option<Self> {
        match text {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        }
    }
}

impl FromText for Decimal {
    const EXPECTED: &'static str = "a decimal number";
    fn from_text(text: &str) -> Option<Self> {
        let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
        let ok = !digits.is_empty()
            && digits.bytes().all(|b| b.is_ascii_digit() || b == b'.')
            && digits.bytes().filter(|&b| b == b'.').count() <= 1
            && digits.bytes().any(|b| b.is_ascii_digit());
        ok.then(|| Decimal(text.to_owned()))
    }
}

fn digits<T: std::str::FromStr>(s: &str, len: usize) -> Option<T> {
    (s.len() == len && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse().ok())?
}

/// Splits a trailing `Z` or `±hh:mm` off `s`.
fn split_offset(s: &str) -> Option<(&str, Option<i16>)> {
    if let Some(rest) = s.strip_suffix('Z') {
        return Some((rest, Some(0)));
    }
    let b = s.as_bytes();
    if b.len() >= 6 && matches!(b[b.len() - 6], b'+' | b'-') && b[b.len() - 3] == b':' {
        let (rest, off) = s.split_at(s.len() - 6);
        let h: i16 = digits(&off[1..3], 2)?;
        let m: i16 = digits(&off[4..6], 2)?;
        if h > 14 || m > 59 {
            return None;
        }
        let total = h * 60 + m;
        return Some((
            rest,
            Some(if off.starts_with('-') { -total } else { total }),
        ));
    }
    Some((s, None))
}

fn parse_date(s: &str) -> Option<Date> {
    let (neg, s) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    let mut parts = s.splitn(3, '-');
    let y = parts.next()?;
    let year: i32 = (y.len() >= 4 && y.len() <= 9 && y.bytes().all(|b| b.is_ascii_digit()))
        .then(|| y.parse().ok())??;
    let month: u8 = digits(parts.next()?, 2)?;
    let day: u8 = digits(parts.next()?, 2)?;
    ((1..=12).contains(&month) && (1..=31).contains(&day)).then_some(Date {
        year: if neg { -year } else { year },
        month,
        day,
    })
}

fn parse_time(s: &str) -> Option<Time> {
    let (hms, frac) = match s.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (s, None),
    };
    if hms.len() != 8 || hms.as_bytes()[2] != b':' || hms.as_bytes()[5] != b':' {
        return None;
    }
    let hour: u8 = digits(&hms[0..2], 2)?;
    let minute: u8 = digits(&hms[3..5], 2)?;
    let second: u8 = digits(&hms[6..8], 2)?;
    let (nanosecond, fraction_digits) = match frac {
        None => (0, 0),
        Some(f) => {
            if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            // Digits beyond nanoseconds are dropped.
            let kept = &f[..f.len().min(9)];
            (
                kept.parse::<u32>().ok()? * 10u32.pow(9 - kept.len() as u32),
                kept.len() as u8,
            )
        }
    };
    (hour <= 24 && minute <= 59 && second <= 60).then_some(Time {
        hour,
        minute,
        second,
        nanosecond,
        fraction_digits,
    })
}

impl std::str::FromStr for DateTime {
    type Err = ParseValueError;

    /// Parses the XML Schema form, for example `2026-01-31T12:00:00Z` or `2026-01-31T13:00:00.5+01:00`.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::from_text(text.trim()).ok_or(ParseValueError(Self::EXPECTED))
    }
}

impl std::str::FromStr for Date {
    type Err = ParseValueError;

    /// Parses the XML Schema form, for example `2026-01-31`.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::from_text(text.trim()).ok_or(ParseValueError(Self::EXPECTED))
    }
}

impl std::str::FromStr for Time {
    type Err = ParseValueError;

    /// Parses the XML Schema form, for example `12:00:00`.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::from_text(text.trim()).ok_or(ParseValueError(Self::EXPECTED))
    }
}

impl std::str::FromStr for Decimal {
    type Err = ParseValueError;

    /// Parses digits with an optional sign and decimal point, for example `12.50`.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::from_text(text.trim()).ok_or(ParseValueError(Self::EXPECTED))
    }
}

/// The text is not a value of the type asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseValueError(&'static str);

impl fmt::Display for ParseValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "expected {}", self.0)
    }
}

impl std::error::Error for ParseValueError {}

impl FromText for DateTime {
    const EXPECTED: &'static str = "a date and time such as 2026-01-31T12:00:00Z";
    fn from_text(text: &str) -> Option<Self> {
        let (date, rest) = text.split_once('T')?;
        let (time, offset_minutes) = split_offset(rest)?;
        Some(DateTime {
            date: parse_date(date)?,
            time: parse_time(time)?,
            offset_minutes,
        })
    }
}

impl FromText for Date {
    const EXPECTED: &'static str = "a date such as 2026-01-31";
    fn from_text(text: &str) -> Option<Self> {
        // A time zone on a bare date carries no usable information here and is dropped.
        let (date, _) = if text.len() > 10 {
            split_offset(text)?
        } else {
            (text, None)
        };
        parse_date(date)
    }
}

impl FromText for Time {
    const EXPECTED: &'static str = "a time such as 12:00:00";
    fn from_text(text: &str) -> Option<Self> {
        let (time, _) = split_offset(text)?;
        parse_time(time)
    }
}

/// Decodes base64, ignoring white space.
pub(crate) fn base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits, mut pad) = (0u32, 0u32, 0u32);
    for b in text.bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                pad += 1;
                continue;
            }
            b' ' | b'\t' | b'\r' | b'\n' => continue,
            _ => return None,
        };
        if pad > 0 {
            return None;
        }
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    (pad <= 2 && acc == 0 && bits < 6).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> DateTime {
        DateTime::from_text(s).unwrap_or_else(|| panic!("{s}"))
    }

    #[test]
    fn date_time_keeps_what_was_written() {
        for s in [
            "2026-11-19T18:22:38Z",
            "2026-11-19T18:22:38.729813995Z",
            "2026-11-19T18:22:38.010Z",
            "2026-11-19T18:22:38.500+01:00",
            "2026-11-19T19:27:02.41+01:00",
            "2026-10-19T00:00:00",
            "1899-12-30T00:00:00+02:00",
            "2026-01-01T00:00:00-05:30",
        ] {
            assert_eq!(dt(s).to_string(), s);
        }
        assert_eq!(
            dt("2026-11-19T18:22:38.729813995Z").time.nanosecond,
            729_813_995
        );
        assert_eq!(dt("2026-11-19T18:22:38+00:00").offset_minutes, Some(0));
        assert_eq!(dt("2026-10-19T00:00:00").offset_minutes, None);
    }

    #[test]
    fn date_time_rejects_nonsense() {
        for s in [
            "",
            "2026-11-19",
            "2026-13-01T00:00:00Z",
            "2026-11-19T25:00:00Z",
            "2026-11-19T10:00Z",
            "2026-11-19T10:00:00.Z",
            "x",
        ] {
            assert!(DateTime::from_text(s).is_none(), "{s}");
        }
    }

    #[test]
    fn unix_timestamp() {
        assert_eq!(dt("1970-01-01T00:00:00Z").unix_timestamp(), Some(0));
        assert_eq!(
            dt("2026-11-19T19:27:02+01:00").unix_timestamp(),
            Some(1_795_112_822)
        );
        assert_eq!(dt("2026-10-19T00:00:00").unix_timestamp(), None);
    }

    #[test]
    fn numbers() {
        assert_eq!(f32::from_text("1.5"), Some(1.5));
        assert_eq!(f64::from_text("-INF"), Some(f64::NEG_INFINITY));
        assert_eq!(f32::from_text("inf"), None);
        assert_eq!(f32::from_text("abc"), None);
        assert_eq!(u64::from_text("-1"), None);
        assert_eq!(i64::from_text("+7"), Some(7));
        assert_eq!(Decimal::from_text("12.50").unwrap().as_str(), "12.50");
        assert!(Decimal::from_text("1e3").is_none());
        assert_eq!(bool::from_text("1"), Some(true));
    }

    #[test]
    fn base64_decodes() {
        assert_eq!(base64("aGVsbG8=").as_deref(), Some(&b"hello"[..]));
        assert_eq!(base64("aGVs\nbG8h").as_deref(), Some(&b"hello!"[..]));
        assert_eq!(base64("a*b"), None);
    }
}
