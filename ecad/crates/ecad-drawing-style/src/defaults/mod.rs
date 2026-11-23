//! The look's defaults: the registry entries of the text styles and the style choices, and their
//! built-in data ([`data`]). The registry's mechanism is [`ecad_model::registry`].

pub mod data;
pub mod styles;
pub mod text;

#[cfg(test)]
mod tests {
    use super::*;
    use ecad_model::registry::{Resolved, Scope, Source, Value, resolve, symbol_key};
    use ecad_model::{Device, DeviceId, Document, Page, PageId, RepresentationId};

    #[test]
    fn hostile_stored_values_fall_through_to_the_next_level() {
        let tag = &text::WIRE_LABEL_HEIGHT;
        let font = &text::FONT;
        let show = &text::TAG_SHOW;
        let long = "x".repeat(10_000);
        for bad in ["NaN", "nan", "inf", "-inf", "1e308", "0.1", "11", "abc", "", "  ", "2.5mm", long.as_str()] {
            assert_eq!(tag.parse(bad), None, "height {bad:?}");
            let mut doc = Document::default();
            doc.edit_parts(|parts| {
                parts.display.insert(tag.key.into(), bad.into());
            });
            assert_eq!(resolve(&doc, tag, Scope::project()), Resolved { value: tag.builtin(), from: Source::Builtin });
        }
        for bad in ["", "a\u{7}b", "line\nbreak", &"f".repeat(65), long.as_str()] {
            assert_eq!(font.parse(bad), None, "font {bad:?}");
        }
        for bad in ["maybe", "first ", "1", ""] {
            if bad == "first " {
                // trimmed: a valid word
                assert_eq!(show.parse(bad), Some(Value::Choice("first")));
            } else {
                assert_eq!(show.parse(bad), None, "show {bad:?}");
            }
        }
        // A hostile page value under a good project value: the project wins.
        let mut doc = Document::default();
        let page = PageId(7);
        doc.edit_parts(|parts| {
            parts.pages.insert(page, Page::titled("p").unwrap());
        });
        doc.edit_parts(|parts| {
            parts.display.insert(show.key.into(), "no".into());
        });
        doc.edit_parts(|parts| {
            if let Some(p) = parts.pages.get_mut(&page) {
                p.display.insert(show.key.into(), "sideways".into());
            }
        });
        assert_eq!(resolve(&doc, show, Scope::page(page)), Resolved { value: Value::Choice("no"), from: Source::Project });
    }

    #[test]
    fn the_narrowest_allowed_level_wins() {
        let show = &text::TAG_SHOW;
        let mut doc = Document::default();
        let page = PageId(1);
        let comp = DeviceId(2);
        let rep = RepresentationId(3);
        doc.edit_parts(|parts| {
            parts.pages.insert(page, Page::titled("p").unwrap());
        });
        doc.edit_parts(|parts| {
            parts.devices.insert(comp, Device::default());
        });
        let at = Scope { page: Some(page), component: Some(comp), rep: Some(rep), conductor: None };
        assert_eq!(resolve(&doc, show, at).from, Source::Builtin);
        doc.edit_parts(|parts| {
            parts.display.insert(show.key.into(), "no".into());
        });
        assert_eq!(resolve(&doc, show, at), Resolved { value: Value::Choice("no"), from: Source::Project });
        doc.edit_parts(|parts| {
            parts.pages.get_mut(&page).map(|p| p.display.insert(show.key.into(), "yes".into()));
        });
        assert_eq!(resolve(&doc, show, at), Resolved { value: Value::Choice("yes"), from: Source::Page });
        doc.edit_parts(|parts| {
            parts.devices.get_mut(&comp).map(|c| c.properties.insert(show.key.into(), "no".into()));
        });
        assert_eq!(resolve(&doc, show, at), Resolved { value: Value::Choice("no"), from: Source::Object });
        doc.edit_parts(|parts| {
            parts.devices.get_mut(&comp).map(|c| c.properties.insert(symbol_key(show.key, rep), "first".into()));
        });
        assert_eq!(resolve(&doc, show, at), Resolved { value: Value::Choice("first"), from: Source::Symbol });
        // The project-only scope still sees only the project.
        assert_eq!(resolve(&doc, show, Scope::project()).from, Source::Project);
        // An entry that does not allow the page level ignores a page value.
        let h = &text::TAG_SHOW_STRIP;
        doc.edit_parts(|parts| {
            parts.pages.get_mut(&page).map(|p| p.display.insert(h.key.into(), "no".into()));
        });
        assert_eq!(resolve(&doc, h, at).from, Source::Builtin);
    }

    #[test]
    fn settings_sources_resolve_as_the_project_level() {
        let e = &text::WIRE_LABEL_HEIGHT;
        let mut doc = Document::default();
        assert_eq!(resolve(&doc, e, Scope::project()).from, Source::Builtin);
        doc.edit_parts(|parts| {
            parts.settings.wiring.label_height = Some(2.0);
        });
        assert_eq!(resolve(&doc, e, Scope::project()), Resolved { value: Value::Length(2.0), from: Source::Project });
        doc.edit_parts(|parts| {
            parts.settings.wiring.label_height = Some(f64::NAN);
        });
        assert_eq!(resolve(&doc, e, Scope::project()).from, Source::Builtin);
    }
}
