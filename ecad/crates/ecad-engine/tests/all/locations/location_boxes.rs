//! A location box keeps its own location style
//! as its aid's field. It draws in it, counts as the style's user, moves with a deleted style's
//! replacement, and `setlocation` on a box names it as a box.

use ecad_electrical::styles;
use ecad_model::{AidId, StyleCatalogue, StyleRef};
use serde_json::json;

use crate::common::{H, id};

fn boxed(h: &mut H) -> AidId {
    id(&h.run("locationbox", json!({"location": "+CP1", "corners": [[60, 60], [140, 140]]})).unwrap(), "aid")
}

#[test]
fn a_box_draws_in_its_own_style_and_is_its_user() {
    let mut h = H::new(0x72f_c201);
    let b = boxed(&mut h);
    let v = h
        .run("setstyle", json!({"catalogue": "location", "new": true, "name": "Field", "copy_of": "box", "fields": {"line_type": "continuous"}}))
        .unwrap();
    let field = StyleRef::parse(v["id"].as_str().unwrap()).unwrap();
    assert_eq!(styles::location::box_draw(&h.doc(), b).linetype, styles::location::linetype("dashed"), "the project's until set");
    h.run("setprop", json!({"aids": [b.hex()], "key": "style", "value": "Field"})).unwrap();
    let d = h.doc();
    assert_eq!(d.aids()[&b].as_box().and_then(|x| x.style.clone()), Some(field.clone()), "stored by ID");
    assert_eq!(styles::location::box_draw(&d, b).linetype, styles::location::linetype("continuous"), "drawn in its own");
    assert_eq!(styles::box_users(&d, StyleCatalogue::Location, &field), vec![b]);
    let c = styles::users(&d, StyleCatalogue::Location, &field);
    assert_eq!((c.explicit, c.implicit), (1, 0));
    let builtin = styles::users(&d, StyleCatalogue::Location, &StyleRef::Builtin("box".into()));
    assert_eq!(builtin.implicit, 0, "a box with its own style is not the project style's");

    // Deleting a style a box uses asks for a replacement, then moves the box to it.
    let e = h.run("deletestyle", json!({"catalogue": "location", "id": field.text()})).unwrap_err().to_string();
    assert!(e.contains("1 location box uses Field"), "{e}");
    h.run("deletestyle", json!({"catalogue": "location", "id": field.text(), "replace_with": "box"})).unwrap();
    assert_eq!(h.doc().aids()[&b].as_box().and_then(|x| x.style.clone()), StyleRef::builtin("box"), "moved to the replacement");

    // Null clears it; a key a box does not have names `style` among its keys.
    h.run("setprop", json!({"aids": [b.hex()], "key": "style", "value": null})).unwrap();
    assert_eq!(h.doc().aids()[&b].as_box().and_then(|x| x.style.clone()), None);
    let e = h.run("setprop", json!({"aids": [b.hex()], "key": "colour", "value": "red"})).unwrap_err().to_string();
    assert!(e.contains("A location box has a `function`, a `location`, a `panel_temperature` and a `style`, not `colour`."), "{e}");
    h.check();
}

#[test]
fn setlocation_on_a_box_names_it_as_a_box() {
    let mut h = H::new(0x72f_c202);
    let b = boxed(&mut h);
    let head = h.head();
    for cmd in ["setlocation", "setfunction"] {
        let e = h.run(cmd, json!({"component": b.hex(), "value": "+CP2"})).unwrap_err().to_string();
        assert!(e.contains(&format!("{} is a location box; edit it with setprop {{aids}}", b.hex())), "{cmd}: {e}");
    }
    assert_eq!(h.head(), head, "nothing written");
}
