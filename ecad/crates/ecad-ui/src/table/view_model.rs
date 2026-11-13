//! The visible order (design §6.2, "View model"): filter → group → sort over a source's keys.
//!
//! Built whole only on [`ViewModel::rebuild`] (first show, reload, a feed that rolled over). A
//! sort or grouping change re-reads one column; a filter keystroke re-reads nothing (each row's
//! search text is kept, lower-cased). [`ViewModel::patch`] re-reads only the touched keys and moves
//! each to its sorted place, by binary search.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use super::source::{Cell, ColumnId, TableSource};

/// Sort direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Asc,
    Desc,
}

/// One visible row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row<K> {
    /// A group heading: the group's text, how many rows it holds (after the filter), and whether
    /// it is open.
    Group {
        label: String,
        count: usize,
        open: bool,
    },
    Item(K),
}

/// What a row is sorted by.
#[derive(Clone, Debug, PartialEq)]
enum SortVal {
    None,
    Num(f64),
    Text(String),
}

impl SortVal {
    fn of(cell: &Cell) -> SortVal {
        match cell {
            Cell::Number(v, _) => SortVal::Num(*v),
            Cell::Empty => SortVal::Text(String::new()),
            c => SortVal::Text(c.text().into_owned()),
        }
    }

    fn cmp(&self, other: &SortVal) -> Ordering {
        match (self, other) {
            (SortVal::None, SortVal::None) => Ordering::Equal,
            (SortVal::Num(a), SortVal::Num(b)) => a.total_cmp(b),
            (SortVal::Text(a), SortVal::Text(b)) => natural_cmp(a, b),
            (SortVal::Num(_), SortVal::Text(_)) => Ordering::Less,
            (SortVal::Text(_), SortVal::Num(_)) => Ordering::Greater,
            (SortVal::None, _) => Ordering::Less,
            (_, SortVal::None) => Ordering::Greater,
        }
    }
}

/// What the view keeps per row: not the values, only what sorts, groups, filters and tints it.
#[derive(Clone, Debug)]
struct Entry<K> {
    key: K,
    /// Its place in the source's order when last read.
    idx: usize,
    group: String,
    sort: SortVal,
    /// Its shown texts, lower-cased, for the search field.
    hay: String,
    /// The chips it passes (bit `i`: chip `i`).
    chips: u64,
    /// Each column's [`Cell::fingerprint`], in column order (the change tint compares them).
    prints: Vec<u64>,
}

/// Natural order: digit runs compare by value, so `-K2` comes before `-K10`; ASCII letters compare
/// without case, then the strings as written, so the order is total. Allocates nothing.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (x, y) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0usize, 0usize);
    loop {
        match (x.get(i).copied(), y.get(j).copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let run = |s: &[u8], from: usize| {
                    let end = s.iter().skip(from).position(|c| !c.is_ascii_digit()).map_or(s.len(), |n| from + n);
                    let start = s.iter().take(end).skip(from).position(|c| *c != b'0').map_or(end, |n| from + n);
                    (start, end)
                };
                let ((sa, ea), (sb, eb)) = (run(x, i), run(y, j));
                let (na, nb) = (x.get(sa..ea).unwrap_or_default(), y.get(sb..eb).unwrap_or_default());
                let o = na.len().cmp(&nb.len()).then_with(|| na.cmp(nb));
                if o != Ordering::Equal {
                    return o;
                }
                (i, j) = (ea, eb);
            }
            (Some(c), Some(d)) => {
                let o = c.to_ascii_lowercase().cmp(&d.to_ascii_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                i += 1;
                j += 1;
            }
        }
    }
}

/// The visible order of a table.
#[derive(Clone, Debug)]
pub struct ViewModel<K> {
    entries: HashMap<K, Entry<K>>,
    /// Every row (filtered or not) in sorted order.
    order: Vec<K>,
    /// The rows shown: filtered, grouped, collapsed groups folded.
    rows: Vec<Row<K>>,
    /// The search text, lower-cased.
    query: String,
    /// The chips switched on (bit `i`).
    chips_on: u64,
    group_by: Option<ColumnId>,
    sort: Option<(ColumnId, Dir)>,
    collapsed: BTreeSet<String>,
    /// Built whole ([`ViewModel::rebuild`]) this many times.
    pub builds: u64,
    /// Keys patched in place.
    pub patches: u64,
    /// Visible rows laid out again (filter, collapse, patch).
    pub layouts: u64,
}

impl<K> Default for ViewModel<K> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            order: Vec::new(),
            rows: Vec::new(),
            query: String::new(),
            chips_on: 0,
            group_by: None,
            sort: None,
            collapsed: BTreeSet::new(),
            builds: 0,
            patches: 0,
            layouts: 0,
        }
    }
}

impl<K: Copy + Ord + std::hash::Hash + std::fmt::Debug> ViewModel<K> {
    /// The rows shown.
    pub fn rows(&self) -> &[Row<K>] {
        &self.rows
    }

    /// The keys shown, in order (group headings left out).
    pub fn visible_keys(&self) -> Vec<K> {
        self.rows
            .iter()
            .filter_map(|r| match r {
                Row::Item(k) => Some(*k),
                Row::Group { .. } => None,
            })
            .collect()
    }

    /// Every row the source has, sorted, before the filter.
    pub fn order(&self) -> &[K] {
        &self.order
    }

    /// Whether the view holds `key` (filtered or not).
    pub fn has(&self, key: K) -> bool {
        self.entries.contains_key(&key)
    }

    pub fn sort(&self) -> Option<(ColumnId, Dir)> {
        self.sort
    }

    pub fn group_by(&self) -> Option<ColumnId> {
        self.group_by
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn chip_on(&self, chip: usize) -> bool {
        chip < 64 && self.chips_on & (1 << chip) != 0
    }

    /// The index of `key`'s row among the shown rows.
    pub fn row_of(&self, key: K) -> Option<usize> {
        self.rows.iter().position(|r| *r == Row::Item(key))
    }

    fn entry<S: TableSource<Key = K>>(&self, src: &S, key: K, idx: usize) -> Entry<K> {
        let cols = src.columns();
        let mut hay = String::new();
        let mut prints = Vec::with_capacity(cols.len());
        let mut group = String::new();
        let mut sort = SortVal::None;
        for c in cols {
            let cell = src.cell(key, c.id);
            prints.push(cell.fingerprint());
            let text = cell.text();
            if !text.is_empty() {
                hay.push_str(&text.to_lowercase());
                hay.push('\u{1f}');
            }
            if self.group_by == Some(c.id) {
                group = text.into_owned();
            }
            if self.sort.is_some_and(|(s, _)| s == c.id) {
                sort = SortVal::of(&cell);
            }
        }
        let mut chips = 0u64;
        for i in 0..src.chips().len().min(64) {
            if src.chip(key, i) {
                chips |= 1 << i;
            }
        }
        Entry { key, idx, group, sort, hay, chips, prints }
    }

    fn cmp_entries(&self, a: &Entry<K>, b: &Entry<K>) -> Ordering {
        let dir = self.sort.map_or(Dir::Asc, |(_, d)| d);
        let by_sort = match dir {
            Dir::Asc => a.sort.cmp(&b.sort),
            Dir::Desc => b.sort.cmp(&a.sort),
        };
        natural_cmp(&a.group, &b.group).then(by_sort).then(a.idx.cmp(&b.idx)).then(a.key.cmp(&b.key))
    }

    fn resort(&mut self) {
        let mut v: Vec<&Entry<K>> = self.order.iter().filter_map(|k| self.entries.get(k)).collect();
        v.sort_by(|a, b| self.cmp_entries(a, b));
        let order = v.iter().map(|e| e.key).collect();
        self.order = order;
    }

    /// Read every row from `src` and lay the view out (first show, reload, `Changed::All`).
    pub fn rebuild<S: TableSource<Key = K>>(&mut self, src: &S) {
        self.entries.clear();
        self.order.clear();
        for (i, k) in src.keys().iter().enumerate() {
            if self.entries.contains_key(k) {
                continue;
            }
            let e = self.entry(src, *k, i);
            self.entries.insert(*k, e);
            self.order.push(*k);
        }
        self.resort();
        self.builds += 1;
        self.layout(src);
    }

    /// Re-read the sort and group texts of every row (only those two columns) and sort again.
    fn reread_order<S: TableSource<Key = K>>(&mut self, src: &S) {
        let (group_by, sort) = (self.group_by, self.sort);
        for e in self.entries.values_mut() {
            e.group = group_by.map(|c| src.cell(e.key, c).text().into_owned()).unwrap_or_default();
            e.sort = sort.map_or(SortVal::None, |(c, _)| SortVal::of(&src.cell(e.key, c)));
        }
        self.resort();
        self.layout(src);
    }

    /// Sort by a column (`None`: the source's order).
    pub fn set_sort<S: TableSource<Key = K>>(&mut self, src: &S, sort: Option<(ColumnId, Dir)>) {
        if self.sort != sort {
            self.sort = sort;
            self.reread_order(src);
        }
    }

    /// Group by a column (`None`: no groups).
    pub fn set_group<S: TableSource<Key = K>>(&mut self, src: &S, group_by: Option<ColumnId>) {
        if self.group_by != group_by {
            self.group_by = group_by;
            self.collapsed.clear();
            self.reread_order(src);
        }
    }

    /// The search text (matched without case against every shown text).
    pub fn set_query<S: TableSource<Key = K>>(&mut self, src: &S, query: &str) {
        let q = query.trim().to_lowercase();
        if q != self.query {
            self.query = q;
            self.layout(src);
        }
    }

    /// Switch chip `chip` on or off.
    pub fn set_chip<S: TableSource<Key = K>>(&mut self, src: &S, chip: usize, on: bool) {
        if chip >= 64 {
            return;
        }
        let mask = if on { self.chips_on | (1 << chip) } else { self.chips_on & !(1 << chip) };
        if mask != self.chips_on {
            self.chips_on = mask;
            self.layout(src);
        }
    }

    /// Open or fold a group.
    pub fn toggle_group<S: TableSource<Key = K>>(&mut self, src: &S, label: &str) {
        if !self.collapsed.remove(label) {
            self.collapsed.insert(label.to_string());
        }
        self.layout(src);
    }

    /// Re-read `keys` from `src` and move each to its sorted place (added, changed, removed): no
    /// other row is read. Returns the cells whose look changed, for the change tint.
    pub fn patch<S: TableSource<Key = K>>(&mut self, src: &S, keys: &[K]) -> Vec<(K, ColumnId)> {
        let mut changed = Vec::new();
        let mut seen = BTreeSet::new();
        for k in keys {
            if !seen.insert(*k) {
                continue;
            }
            let old = self.entries.remove(k);
            if let Some(old) = &old {
                // Its place under the old values, found by binary search (falls back to a scan).
                let at = self.order.binary_search_by(|x| if x == k { Ordering::Equal } else { self.cmp_with(x, old) }).ok();
                if let Some(i) = at.filter(|i| self.order.get(*i) == Some(k)).or_else(|| self.order.iter().position(|x| x == k)) {
                    self.order.remove(i);
                }
            }
            self.patches += 1;
            if !src.contains(*k) {
                continue;
            }
            let idx = src.keys().iter().position(|x| x == k).unwrap_or(usize::MAX);
            let new = self.entry(src, *k, idx);
            if let Some(old) = &old {
                let cols = src.columns();
                for (i, (a, b)) in old.prints.iter().zip(&new.prints).enumerate() {
                    if a != b
                        && let Some(c) = cols.get(i)
                    {
                        changed.push((*k, c.id));
                    }
                }
            }
            let at = self.order.partition_point(|x| self.entries.get(x).is_some_and(|e| self.cmp_entries(e, &new) == Ordering::Less));
            self.order.insert(at, *k);
            self.entries.insert(*k, new);
        }
        self.layout(src);
        changed
    }

    /// How `x` (a held row) compares with the entry `e` (not held any more).
    fn cmp_with(&self, x: &K, e: &Entry<K>) -> Ordering {
        self.entries.get(x).map_or(Ordering::Less, |xe| self.cmp_entries(xe, e))
    }

    fn passes<S: TableSource<Key = K>>(&self, src: &S, e: &Entry<K>) -> bool {
        if e.chips & self.chips_on != self.chips_on {
            return false;
        }
        if self.query.is_empty() {
            return true;
        }
        src.matches(e.key, &self.query).unwrap_or_else(|| e.hay.contains(&self.query))
    }

    /// Lay the shown rows out from the sorted order: filter, then group headings, folding closed
    /// groups. Reads nothing from the source but its own match rule.
    fn layout<S: TableSource<Key = K>>(&mut self, src: &S) {
        let mut rows = Vec::with_capacity(self.order.len());
        let grouped = self.group_by.is_some();
        let mut current: Option<(usize, String)> = None;
        let mut count = 0usize;
        fn close<K>(rows: &mut [Row<K>], current: &Option<(usize, String)>, count: usize) {
            if let Some((at, _)) = current
                && let Some(Row::Group { count: c, .. }) = rows.get_mut(*at)
            {
                *c = count;
            }
        }
        for k in &self.order {
            let Some(e) = self.entries.get(k) else { continue };
            if !self.passes(src, e) {
                continue;
            }
            if grouped {
                if current.as_ref().is_none_or(|(_, g)| *g != e.group) {
                    close(&mut rows, &current, count);
                    count = 0;
                    let open = !self.collapsed.contains(&e.group);
                    current = Some((rows.len(), e.group.clone()));
                    rows.push(Row::Group { label: e.group.clone(), count: 0, open });
                }
                count += 1;
                if self.collapsed.contains(&e.group) {
                    continue;
                }
            }
            rows.push(Row::Item(*k));
        }
        close(&mut rows, &current, count);
        self.rows = rows;
        self.layouts += 1;
    }
}
