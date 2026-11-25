//! Which page an undo or redo brings to the front.

use std::collections::BTreeSet;

use ecad_model::PageId;

/// The page an undo or redo brings to the front, or `None` to stay: a page it
/// adds; the page before the active page when it removes the active page (else the first page);
/// otherwise, when it changed pages that do not include the active page, the first of them in
/// page order. Rename and move (a page removed and added again) stay; so does a change that could
/// be on any page (`touched` `None`), and reference texts changing elsewhere never switch.
pub fn front_after_history(before: &[PageId], after: &[PageId], touched: Option<&BTreeSet<PageId>>, active: Option<PageId>) -> Option<PageId> {
    if let Some(added) = after.iter().find(|p| !before.contains(p)) {
        return Some(*added);
    }
    if let Some(a) = active
        && !after.contains(&a)
    {
        let at = before.iter().position(|p| *p == a).unwrap_or(0);
        let earlier = before.get(..at).unwrap_or_default().iter().rev().find(|p| after.contains(p));
        return earlier.or(after.first()).copied();
    }
    let touched = touched?;
    if active.is_some_and(|a| touched.contains(&a)) {
        return None;
    }
    after.iter().find(|p| touched.contains(p)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Table-driven: which page an undo or redo brings to the front.
    #[test]
    fn the_page_an_undo_or_redo_brings_to_the_front() {
        let [a, b, c] = [PageId(1), PageId(2), PageId(3)];
        let set = |p: &[PageId]| p.iter().copied().collect::<BTreeSet<_>>();
        let cases: &[(&str, &[PageId], &[PageId], Option<BTreeSet<PageId>>, Option<PageId>, Option<PageId>)] = &[
            ("edit on the active page stays", &[a, b], &[a, b], Some(set(&[a])), Some(a), None),
            ("edit on another page switches", &[a, b], &[a, b], Some(set(&[b])), Some(a), Some(b)),
            ("first touched in page order", &[a, b, c], &[a, b, c], Some(set(&[c, b])), Some(a), Some(b)),
            ("nothing drawn changed stays", &[a, b], &[a, b], Some(set(&[])), Some(a), None),
            ("unknown pages stay", &[a, b], &[a, b], None, Some(a), None),
            ("a page added becomes active", &[a, b], &[a, b, c], Some(set(&[])), Some(a), Some(c)),
            ("active removed: the page before", &[a, b, c], &[a, b], Some(set(&[])), Some(c), Some(b)),
            ("active first removed: the first", &[a, b], &[b], Some(set(&[])), Some(a), Some(b)),
            ("rename or move stays", &[a, b], &[b, a], Some(set(&[])), Some(a), None),
            ("no window: touched page", &[a, b], &[a, b], Some(set(&[b])), None, Some(b)),
        ];
        for (what, before, after, touched, active, want) in cases {
            assert_eq!(front_after_history(before, after, touched.as_ref(), *active), *want, "{what}");
        }
    }
}
