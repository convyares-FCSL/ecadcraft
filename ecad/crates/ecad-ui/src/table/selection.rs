//! Multi-select (design §6.2): a set of keys, so it survives sort and filter. Click selects one,
//! Ctrl-click toggles, Shift-click selects the range from the anchor in the shown order, Ctrl+A
//! selects every shown row, ↑ and ↓ move (with Shift, extend).

use std::collections::BTreeSet;

/// The selected rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection<K: Ord> {
    keys: BTreeSet<K>,
    /// Where a Shift-click range starts.
    anchor: Option<K>,
    /// The row the keyboard is on.
    cursor: Option<K>,
}

impl<K: Ord> Default for Selection<K> {
    fn default() -> Self {
        Self { keys: BTreeSet::new(), anchor: None, cursor: None }
    }
}

impl<K: Copy + Ord> Selection<K> {
    pub fn keys(&self) -> &BTreeSet<K> {
        &self.keys
    }

    pub fn contains(&self, k: K) -> bool {
        self.keys.contains(&k)
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn cursor(&self) -> Option<K> {
        self.cursor
    }

    /// The selection in the shown order (rows filtered out are kept but not listed first).
    pub fn in_order(&self, shown: &[K]) -> Vec<K> {
        let mut out: Vec<K> = shown.iter().copied().filter(|k| self.keys.contains(k)).collect();
        out.extend(self.keys.iter().copied().filter(|k| !shown.contains(k)));
        out
    }

    /// Select only `k`.
    pub fn click(&mut self, k: K) {
        self.keys.clear();
        self.keys.insert(k);
        self.anchor = Some(k);
        self.cursor = Some(k);
    }

    /// Add or remove `k`.
    pub fn toggle(&mut self, k: K) {
        if !self.keys.remove(&k) {
            self.keys.insert(k);
        }
        self.anchor = Some(k);
        self.cursor = Some(k);
    }

    /// Select from the anchor to `k` in `shown`'s order (only `k` when there is no anchor shown).
    pub fn extend_to(&mut self, k: K, shown: &[K]) {
        let from = self.anchor.and_then(|a| shown.iter().position(|x| *x == a));
        let to = shown.iter().position(|x| *x == k);
        match (from, to) {
            (Some(a), Some(b)) => {
                self.keys.clear();
                let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                self.keys.extend(shown.iter().skip(lo).take(hi - lo + 1).copied());
                self.cursor = Some(k);
            }
            _ => self.click(k),
        }
    }

    /// Select every shown row.
    pub fn all(&mut self, shown: &[K]) {
        self.keys = shown.iter().copied().collect();
        self.anchor = shown.first().copied();
        self.cursor = shown.last().copied();
    }

    /// Move the cursor `by` rows in `shown` (−1 up, +1 down), selecting it; with `extend`, the
    /// range from the anchor. Returns the new cursor.
    pub fn step(&mut self, shown: &[K], by: isize, extend: bool) -> Option<K> {
        let at = self.cursor.and_then(|c| shown.iter().position(|x| *x == c));
        let next = match at {
            Some(i) => i.saturating_add_signed(by).min(shown.len().saturating_sub(1)),
            None if by < 0 => shown.len().saturating_sub(1),
            None => 0,
        };
        let k = *shown.get(next)?;
        if extend {
            self.extend_to(k, shown)
        } else {
            self.click(k)
        }
        Some(k)
    }

    /// Set the selection to `keys` (from the page), the cursor on the first.
    pub fn set(&mut self, keys: impl IntoIterator<Item = K>) {
        self.keys = keys.into_iter().collect();
        self.anchor = self.keys.first().copied();
        self.cursor = self.anchor;
    }

    /// Drop keys that are no longer rows.
    pub fn retain(&mut self, mut alive: impl FnMut(K) -> bool) {
        self.keys.retain(|k| alive(*k));
        if self.anchor.is_some_and(|k| !alive(k)) {
            self.anchor = None;
        }
        if self.cursor.is_some_and(|k| !alive(k)) {
            self.cursor = None;
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
