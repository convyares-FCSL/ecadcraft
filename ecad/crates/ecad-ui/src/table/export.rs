//! CSV export (design §6.2): the shown rows and columns, as the table shows them.

use super::source::{Column, TableSource};

/// One CSV field, quoted when it holds a comma, a quote or a line break.
fn field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) { format!("\"{}\"", text.replace('"', "\"\"")) } else { text.to_string() }
}

/// The CSV of `keys` (in that order) over `columns`, with a header line; lines end with `\r\n`.
pub fn csv<S: TableSource>(src: &S, columns: &[Column], keys: &[S::Key]) -> String {
    let mut out = String::new();
    let line = |cells: Vec<String>| cells.join(",") + "\r\n";
    out.push_str(&line(columns.iter().map(|c| field(c.title)).collect()));
    for k in keys {
        out.push_str(&line(columns.iter().map(|c| field(&src.cell(*k, c.id).text())).collect()));
    }
    out
}
