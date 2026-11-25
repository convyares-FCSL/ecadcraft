//! Strict input: parameters that are not an object are refused
//! by every typed command with one wording, and nothing is recorded. `null` reads as none.

use ecad_electrical::commands::input::NOT_AN_OBJECT;
use ecad_engine::commands::typed_commands;
use serde_json::json;

use crate::common::H;

#[test]
fn every_typed_command_refuses_parameters_that_are_not_an_object() {
    let mut h = H::new(7099);
    let before = h.doc();
    for t in typed_commands() {
        for v in [json!([1, 2, 3]), json!("x"), json!(5)] {
            assert_eq!(h.run(t.id, v.clone()).err().as_deref(), Some(NOT_AN_OBJECT), "{} {v}", t.id);
        }
    }
    assert!(h.doc() == before, "a refused command changed the project");
}
