//! Tag tables (brief 056, design 056 §7): the format layer and its one format in 056, the generic
//! CSV tag table.
//!
//! # Semantics
//! - Every format reads into and writes from [`TagRow`]; sync and Compare see only rows. A format
//!   is one [`TagFormat`] and one line in [`FORMATS`] (0.5.0 adds the vendor formats that way).
//! - Readers are bounded the same way by [`Caps`]: the file's size, the I/O tags kept, the name
//!   and comment lengths. Only tags with an I/O address are kept ([`normalise`]); memory,
//!   data-block and timer tags, rows with a bad address, too long a name and an address given twice
//!   are skipped and counted in [`Read`].
//! - Addresses are compared normalised (design 056 §6.3): `%`, spaces and `:P` removed, the German
//!   mnemonics `E`/`A` (and `PE`/`PA`) read as `I`/`Q`, the IEC bit letter `X` dropped, upper case:
//!   `%IX0.0`, `E0.0` and `I0.0` are all `I0.0`; `%IW64` is `IW64`.
//! - Writing puts addresses in `%` form (`%Q2.2`). Fields are quoted per RFC 4180 when they hold a
//!   separator, a quote or a line break.
//!
//! Pure functions; no I/O. Never panics on any input.

use std::collections::BTreeSet;

/// One tag of a tag table.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TagRow {
    pub name: String,
    /// Normalised (`I0.0`, `QW64`).
    pub address: String,
    pub data_type: String,
    pub comment: String,
    /// Export-only columns (design 056 §7.3), ignored on import.
    pub extra: Option<Extra>,
}

/// The export-only columns of the generic CSV.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Extra {
    pub card: String,
    pub channel: String,
    pub safe: String,
    pub drives: String,
    pub page: String,
}

/// The limits every reader keeps (design 056 §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    /// Most bytes read.
    pub bytes: usize,
    /// Most I/O tags kept; more is refused.
    pub rows: usize,
    /// Longest name; longer rows are skipped.
    pub name: usize,
    /// Longest comment; longer ones are cut.
    pub comment: usize,
}

impl Default for Caps {
    fn default() -> Self {
        Caps { bytes: MAX_BYTES, rows: MAX_ROWS, name: super::signals::MAX_TAG, comment: MAX_COMMENT }
    }
}

/// Largest file read (16 MB).
pub const MAX_BYTES: usize = 16 << 20;
/// Most I/O tags kept.
pub const MAX_ROWS: usize = 20_000;
/// Longest comment kept.
pub const MAX_COMMENT: usize = 256;
/// Longest data type kept.
pub const MAX_TYPE: usize = 32;
/// Most fields read in one row (a hostile line of separators is cut there).
pub const MAX_FIELDS: usize = 64;

/// What a reader read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Read {
    /// The I/O tags kept, in file order.
    pub rows: Vec<TagRow>,
    /// Data rows in the file (not the header, not blank lines).
    pub total: usize,
    /// Tags that are not I/O (memory, data block, timers, no address).
    pub not_io: usize,
    /// Rows with an address that looks like I/O but is not one (`%Q2.9`, `%Q-1.0`), or broken.
    pub bad: usize,
    /// Rows whose name is empty, too long or has a control character.
    pub bad_name: usize,
    /// Rows at an address an earlier row has.
    pub twice: usize,
}

impl Read {
    /// `412 tags read; 96 with an I/O address. 316 memory and data-block tags skipped.` and the
    /// other skipped counts.
    pub fn words(&self) -> String {
        let mut s = format!("{} {} read; {} with an I/O address.", self.total, plural(self.total, "tag", "tags"), self.rows.len());
        if self.not_io > 0 {
            s.push_str(&format!(" {} memory and data-block {} skipped.", self.not_io, plural(self.not_io, "tag", "tags")));
        }
        if self.bad > 0 {
            s.push_str(&format!(" {} {} with an address that is not I/O were skipped.", self.bad, plural(self.bad, "row", "rows")));
        }
        if self.bad_name > 0 {
            s.push_str(&format!(" {} {} with an empty, too long or broken name were skipped.", self.bad_name, plural(self.bad_name, "row", "rows")));
        }
        if self.twice > 0 {
            s.push_str(&format!(" {} {} at an address given before were skipped.", self.twice, plural(self.twice, "row", "rows")));
        }
        s
    }
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

/// A tag table format (design 056 §7.2).
pub trait TagFormat: Sync {
    /// Its ID in `plcexport {format}` and `plcimport {format}`.
    fn id(&self) -> &'static str;
    /// Its name in the path dialog.
    fn label(&self) -> &'static str;
    /// Its file extensions, without the dot.
    fn extensions(&self) -> &'static [&'static str];
    /// Whether `bytes` look like this format.
    fn sniff(&self, bytes: &[u8]) -> bool;
    /// Read a file.
    ///
    /// # Errors
    /// The file is not this format, too large, or has too many I/O tags; in words.
    fn read(&self, bytes: &[u8], caps: &Caps) -> Result<Read, String>;
    /// Write rows.
    ///
    /// # Errors
    /// A row the format cannot hold.
    fn write(&self, rows: &[TagRow]) -> Result<Vec<u8>, String>;
    /// Whether the format can hold this name (design 056 §7.4); a name it cannot is left out of an
    /// export and counted.
    fn name_ok(&self, name: &str) -> bool {
        !name.is_empty() && name.chars().count() <= super::signals::MAX_TAG && !name.chars().any(char::is_control)
    }
}

/// The generic CSV tag table (design 056 §7.3).
pub struct GenericCsv;

/// The formats, in the order the path dialog lists them.
pub static FORMATS: &[&dyn TagFormat] = &[&GenericCsv];

/// Said when no format reads a file (design 056 §7.1).
pub const UNKNOWN: &str = "Only the generic CSV tag table can be read yet (name, address, type, comment).";

/// The format of ID `id`.
pub fn format(id: &str) -> Option<&'static dyn TagFormat> {
    FORMATS.iter().copied().find(|f| f.id().eq_ignore_ascii_case(id.trim()))
}

/// The format that reads `bytes`, if any.
pub fn sniff(bytes: &[u8]) -> Option<&'static dyn TagFormat> {
    FORMATS.iter().copied().find(|f| f.sniff(bytes))
}

/// The columns of the generic CSV, in order; the last five on export only.
pub const CSV_COLUMNS: [&str; 9] = ["name", "address", "type", "comment", "card", "channel", "safe", "drives", "page"];

/// The header line's fields of a CSV text (BOM removed), with its separator.
fn header(text: &str) -> (Vec<String>, char) {
    let first = text.lines().next().unwrap_or_default();
    let sep = if first.contains(';') && !first.contains(',') { ';' } else { ',' };
    let fields = first.split(sep).take(MAX_FIELDS).map(|f| f.trim().trim_matches('"').trim().to_ascii_lowercase()).collect();
    (fields, sep)
}

/// The text of `bytes` without a UTF-8 BOM, if it is UTF-8 with no NUL.
fn text_of(bytes: &[u8]) -> Option<&str> {
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes);
    if b.contains(&0) {
        return None;
    }
    std::str::from_utf8(b).ok()
}

impl TagFormat for GenericCsv {
    fn id(&self) -> &'static str {
        "csv"
    }

    fn label(&self) -> &'static str {
        "Tag table (.csv)"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["csv", "txt"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = bytes.get(..bytes.len().min(4096)).unwrap_or_default();
        // A cut in the middle of a character is fine for the header line.
        let t = match text_of(head) {
            Some(t) => t,
            None => match std::str::from_utf8(head.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(head)) {
                Ok(t) => t,
                Err(e) => std::str::from_utf8(head.get(..e.valid_up_to()).unwrap_or_default()).unwrap_or_default(),
            },
        };
        if t.contains('\0') {
            return false;
        }
        let (h, _) = header(t);
        h.iter().any(|f| f == "name") && h.iter().any(|f| f == "address")
    }

    fn read(&self, bytes: &[u8], caps: &Caps) -> Result<Read, String> {
        if bytes.len() > caps.bytes {
            return Err(format!("The file is {} MB; at most {} MB is read.", bytes.len() >> 20, caps.bytes >> 20));
        }
        let text = text_of(bytes).ok_or(UNKNOWN)?;
        if text.trim().is_empty() {
            return Err("The file is empty.".into());
        }
        let (head, sep) = header(text);
        let col = |name: &str| head.iter().position(|f| f == name);
        if col("name").is_none() && col("address").is_none() {
            return Err(UNKNOWN.into());
        }
        let name_at = col("name").ok_or("The file has no \"name\" column.")?;
        let addr_at = col("address").ok_or("The file has no \"address\" column.")?;
        let type_at = col("type");
        let comment_at = col("comment");
        let mut out = Read::default();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut first = true;
        let mut over = 0usize;
        for rec in Records::new(text, sep) {
            if first {
                first = false;
                continue;
            }
            let (fields, broken) = rec;
            if fields.iter().all(|f| f.trim().is_empty()) {
                continue;
            }
            out.total += 1;
            if broken {
                out.bad += 1;
                continue;
            }
            let get = |k: Option<usize>| k.and_then(|k| fields.get(k)).map(|s| s.trim()).unwrap_or_default();
            let raw = get(Some(addr_at));
            let address = match normalise(raw) {
                Address::Io(a) => a,
                Address::NotIo => {
                    out.not_io += 1;
                    continue;
                }
                Address::Bad => {
                    out.bad += 1;
                    continue;
                }
            };
            let name = get(Some(name_at));
            if name.is_empty() || name.chars().count() > caps.name || name.chars().any(char::is_control) {
                out.bad_name += 1;
                continue;
            }
            if !seen.insert(address.clone()) {
                out.twice += 1;
                continue;
            }
            if out.rows.len() >= caps.rows {
                // Count the rest so the refusal says how many there were.
                over += 1;
                continue;
            }
            let clip = |s: &str, n: usize| s.chars().filter(|c| !c.is_control()).take(n).collect::<String>();
            out.rows.push(TagRow {
                name: name.to_string(),
                address,
                data_type: clip(get(type_at), MAX_TYPE),
                comment: clip(get(comment_at), caps.comment),
                extra: None,
            });
        }
        if over > 0 {
            return Err(format!("The file has {} I/O tags; at most {} are read.", out.rows.len().saturating_add(over), caps.rows));
        }
        Ok(out)
    }

    fn write(&self, rows: &[TagRow]) -> Result<Vec<u8>, String> {
        let extra = rows.iter().any(|r| r.extra.is_some());
        let cols = if extra { &CSV_COLUMNS[..] } else { &CSV_COLUMNS[..4] };
        let mut s = cols.join(",");
        s.push_str("\r\n");
        for r in rows {
            let mut f: Vec<String> = vec![r.name.clone(), percent(&r.address), r.data_type.clone(), r.comment.clone()];
            if extra {
                let e = r.extra.clone().unwrap_or_default();
                f.extend([e.card, e.channel, e.safe, e.drives, e.page]);
            }
            let line: Vec<String> = f.iter().map(|x| quote(x)).collect();
            s.push_str(&line.join(","));
            s.push_str("\r\n");
        }
        Ok(s.into_bytes())
    }
}

/// A CSV field, quoted when it holds a separator, a quote or a line break (RFC 4180).
pub fn quote(s: &str) -> String {
    if s.contains([',', ';', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// The records of a CSV text (RFC 4180 quoting, `sep` the separator): each record's fields and
/// whether it was broken (an unbalanced quote runs to the end of the text).
struct Records<'a> {
    rest: &'a str,
    sep: char,
}

impl<'a> Records<'a> {
    fn new(text: &'a str, sep: char) -> Self {
        Records { rest: text, sep }
    }
}

impl Iterator for Records<'_> {
    type Item = (Vec<String>, bool);

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        let mut fields = Vec::new();
        let mut field = String::new();
        let mut quoted = false;
        let mut chars = self.rest.char_indices().peekable();
        let mut end = self.rest.len();
        let mut at_start = true;
        while let Some((i, c)) = chars.next() {
            if quoted {
                if c == '"' {
                    if chars.peek().is_some_and(|(_, n)| *n == '"') {
                        field.push('"');
                        chars.next();
                    } else {
                        quoted = false;
                    }
                } else if field.len() <= MAX_BYTES {
                    field.push(c);
                }
                continue;
            }
            if c == '"' && at_start {
                quoted = true;
                at_start = false;
                continue;
            }
            if c == self.sep {
                if fields.len() < MAX_FIELDS {
                    fields.push(std::mem::take(&mut field));
                } else {
                    field.clear();
                }
                at_start = true;
                continue;
            }
            if c == '\n' || c == '\r' {
                end = i + c.len_utf8();
                if c == '\r' && chars.peek().is_some_and(|(_, n)| *n == '\n') {
                    end += 1;
                }
                break;
            }
            if !(at_start && c.is_whitespace()) {
                at_start = false;
            }
            field.push(c);
        }
        if fields.len() < MAX_FIELDS {
            fields.push(field);
        }
        // A quote never closed swallowed the rest of the text: the record is broken.
        let broken = quoted;
        self.rest = if broken { "" } else { self.rest.get(end..).unwrap_or_default() };
        Some((fields, broken))
    }
}

/// An address read from a tag table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    /// An I/O address, normalised.
    Io(String),
    /// Not an I/O address (memory, data block, timer, empty).
    NotIo,
    /// Shaped like an I/O address, but not one (`Q2.9`, `Q-1.0`, a byte past the end).
    Bad,
}

/// Normalise an address (design 056 §6.3).
pub fn normalise(raw: &str) -> Address {
    let mut s: String = raw.chars().filter(|c| !c.is_whitespace() && *c != '%').collect::<String>().to_ascii_uppercase();
    if s.is_empty() || s.len() > 32 {
        return if s.is_empty() { Address::NotIo } else { Address::Bad };
    }
    if let Some(x) = s.strip_suffix(":P") {
        s = x.to_string();
    }
    // Periphery prefix and German mnemonics.
    let (dir, rest) = if let Some(r) = s.strip_prefix("PE").or_else(|| s.strip_prefix("PI")) {
        ('I', r)
    } else if let Some(r) = s.strip_prefix("PA").or_else(|| s.strip_prefix("PQ")) {
        ('Q', r)
    } else if let Some(r) = s.strip_prefix('I').or_else(|| s.strip_prefix('E')) {
        ('I', r)
    } else if let Some(r) = s.strip_prefix('Q').or_else(|| s.strip_prefix('A')) {
        ('Q', r)
    } else {
        return Address::NotIo;
    };
    // The size letter: X (bit, dropped), B, W, D; nothing is a bit address.
    let (size, num) = match rest.chars().next() {
        Some('X') => ("", rest.get(1..).unwrap_or_default()),
        Some(c @ ('B' | 'W' | 'D')) => (
            if c == 'B' {
                "B"
            } else if c == 'W' {
                "W"
            } else {
                "D"
            },
            rest.get(1..).unwrap_or_default(),
        ),
        Some(c) if c.is_ascii_digit() => ("", rest),
        // A sign after the direction: shaped like I/O, but no address.
        Some('-' | '+') => return Address::Bad,
        // `AB…`, `IN…`, anything else after I/Q/E/A: not an I/O address.
        _ => return Address::NotIo,
    };
    let digits = |t: &str| !t.is_empty() && t.len() <= 6 && t.bytes().all(|b| b.is_ascii_digit());
    let max = super::address::MAX_BYTE;
    if size.is_empty() {
        let Some((byte, bit)) = num.split_once('.') else { return Address::Bad };
        if !digits(byte) || bit.len() != 1 || !digits(bit) {
            return Address::Bad;
        }
        let (Ok(b), Ok(t)) = (byte.parse::<u32>(), bit.parse::<u32>()) else { return Address::Bad };
        if b > max || t > 7 {
            return Address::Bad;
        }
        return Address::Io(format!("{dir}{b}.{t}"));
    }
    if !digits(num) {
        return Address::Bad;
    }
    match num.parse::<u32>() {
        Ok(b) if b <= max => Address::Io(format!("{dir}{size}{b}")),
        _ => Address::Bad,
    }
}

/// The normalised form of an ECAD address (any 045 style but `slot` and `none`), if it is I/O.
pub fn key_of(address: &str) -> Option<String> {
    match normalise(address) {
        Address::Io(a) => Some(a),
        _ => None,
    }
}

/// `%Q2.2` from `Q2.2`.
pub fn percent(normalised: &str) -> String {
    if normalised.is_empty() || normalised.starts_with('%') { normalised.to_string() } else { format!("%{normalised}") }
}

/// The sort key of a normalised address: direction, size, byte, bit, so `I2.0` comes before
/// `I10.0`.
pub fn order(a: &str) -> (char, u8, u32, u32) {
    let mut ch = a.chars();
    let dir = ch.next().unwrap_or(' ');
    let rest: String = ch.collect();
    let (size, num) = match rest.chars().next() {
        Some('B') => (1, rest.get(1..).unwrap_or_default().to_string()),
        Some('W') => (2, rest.get(1..).unwrap_or_default().to_string()),
        Some('D') => (3, rest.get(1..).unwrap_or_default().to_string()),
        _ => (0, rest.clone()),
    };
    let (b, t) = num.split_once('.').unwrap_or((num.as_str(), "0"));
    (dir, size, b.parse().unwrap_or(u32::MAX), t.parse().unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_normalise() {
        for (raw, want) in [
            ("%Q2.2", "Q2.2"),
            ("%IX0.0", "I0.0"),
            ("E0.1", "I0.1"),
            ("A3.0", "Q3.0"),
            ("%IW64", "IW64"),
            ("PEW256", "IW256"),
            ("%Q 2.2 :P", "Q2.2"),
            ("q2.2", "Q2.2"),
        ] {
            assert_eq!(normalise(raw), Address::Io(want.into()), "{raw}");
        }
        for raw in ["%M0.0", "DB1.DBX0.0", "T1", "", "MW10"] {
            assert_eq!(normalise(raw), Address::NotIo, "{raw}");
        }
        for raw in ["%Q99999.9", "%Q2.9", "%Q-1.0", "%Q2", "%IW", "%Q2.2.2"] {
            assert_eq!(normalise(raw), Address::Bad, "{raw}");
        }
    }

    #[test]
    fn csv_round_trips() {
        let rows = vec![
            TagRow { name: "Valve, \"inlet\"".into(), address: "Q2.2".into(), data_type: "Bool".into(), comment: "Line\nbreak".into(), extra: None },
            TagRow { name: "Level".into(), address: "IW64".into(), data_type: "Int".into(), comment: String::new(), extra: None },
        ];
        let bytes = GenericCsv.write(&rows).unwrap_or_default();
        assert!(GenericCsv.sniff(&bytes));
        let back = GenericCsv.read(&bytes, &Caps::default()).unwrap_or_default();
        let names: Vec<&str> = back.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Valve, \"inlet\"", "Level"]);
        assert_eq!(back.rows.first().map(|r| r.comment.as_str()), Some("Linebreak"));
    }

    #[test]
    fn semicolons_and_a_bom_read() {
        let t = "\u{feff}Name;Address;Type;Comment\r\nA;%Q2.2;Bool;x\r\nB;%M0.0;Bool;\r\n";
        let r = GenericCsv.read(t.as_bytes(), &Caps::default()).unwrap_or_default();
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.not_io, 1);
    }

    #[test]
    fn unbalanced_quotes_and_garbage_never_panic() {
        let cases: Vec<Vec<u8>> = vec![
            b"name,address\n\"open,%Q0.0\n".to_vec(),
            vec![0, 159, 146, 150],
            b"".to_vec(),
            b"PK\x03\x04garbage".to_vec(),
            ",,,,,,,,\n".repeat(10).into_bytes(),
        ];
        for c in cases {
            let _ = GenericCsv.read(&c, &Caps::default());
            let _ = GenericCsv.sniff(&c);
        }
    }
}
