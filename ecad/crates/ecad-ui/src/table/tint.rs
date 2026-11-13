//! The change tint (design §8.3, spec §5): a cell whose look changed in a commit is tinted for
//! [`TINT_SECONDS`], fading. Only touched keys are compared, so only changed cells are marked.

use std::collections::HashMap;

use super::source::ColumnId;

/// How long a changed cell stays tinted.
pub const TINT_SECONDS: f64 = 1.2;

/// When each changed cell changed (egui's input time, seconds).
#[derive(Clone, Debug)]
pub struct TintMap<K> {
    at: HashMap<(K, ColumnId), f64>,
}

impl<K> Default for TintMap<K> {
    fn default() -> Self {
        Self { at: HashMap::new() }
    }
}

impl<K: Copy + Eq + std::hash::Hash> TintMap<K> {
    /// Mark cells changed at `now`.
    pub fn mark(&mut self, cells: impl IntoIterator<Item = (K, ColumnId)>, now: f64) {
        for c in cells {
            self.at.insert(c, now);
        }
    }

    /// The tint of a cell at `now`: 1 just changed, falling to 0 at [`TINT_SECONDS`].
    pub fn level(&self, key: K, col: ColumnId, now: f64) -> f32 {
        match self.at.get(&(key, col)) {
            Some(t) if now >= *t && now - *t < TINT_SECONDS => (1.0 - (now - *t) / TINT_SECONDS) as f32,
            _ => 0.0,
        }
    }

    /// Whether any cell is still tinted (the table asks for repaints only then).
    pub fn live(&self, now: f64) -> bool {
        self.at.values().any(|t| now - *t < TINT_SECONDS)
    }

    /// Forget tints that have run out.
    pub fn prune(&mut self, now: f64) {
        self.at.retain(|_, t| now - *t < TINT_SECONDS);
    }

    /// The cells tinted at `now`.
    pub fn tinted(&self, now: f64) -> Vec<(K, ColumnId)> {
        self.at.iter().filter(|(_, t)| now - **t < TINT_SECONDS).map(|(k, _)| *k).collect()
    }

    pub fn clear(&mut self) {
        self.at.clear();
    }
}
