//! The built-in coil: the definition `place` uses without a symbol, outside the starter catalogue.
//!
//! # Semantics
//! Like a starter symbol it carries a fixed [`Origin`] (its own library, definition 1, `library_seq` 1 and its
//! content hash), and a project's embedded copy is found by that origin, never by its name: a symbol of the
//! person's own called "Two-point coil" is never taken for it.
//!
//! Excluded: placing it (the engine's `place`).

use ecad_model::{ConnectionPointDef, Definition, DefinitionId, LibraryId, Origin, Primitive, Pt};

use crate::CmdError;

/// The library ID of the built-in definitions that are not starters (the coil `place` uses without a symbol): one
/// fixed ID.
pub const BUILTIN_LIBRARY: LibraryId = LibraryId(0x0000_ecad_0e46_b111_0000_0000_0000_0001);

/// The coil's fixed definition ID inside [`BUILTIN_LIBRARY`].
pub const BUILTIN_COIL: DefinitionId = DefinitionId(1);

/// Whether `d` is this build's built-in coil, `coil` ([`builtin_coil`]): by its origin (library, definition and
/// content hash), never by its name, so a symbol of the person's own named "Two-point coil" is never taken for it.
pub fn is_builtin_coil(d: &Definition, coil: &Definition) -> bool {
    let want = coil.origin.as_ref().map(|o| o.content_hash);
    d.origin.as_ref().is_some_and(|o| o.library == BUILTIN_LIBRARY && o.definition == BUILTIN_COIL && Some(o.content_hash) == want)
}

/// The built-in definition `place` uses without a symbol, saved as **Two-point coil**: 10 × 20,
/// connection points A1 (top) and A2 (bottom). Its origin is [`BUILTIN_COIL`] in [`BUILTIN_LIBRARY`] with its
/// content hash; `place` finds it again by that origin ([`is_builtin_coil`]).
///
/// # Errors
/// Its content hash could not be computed (not for this fixed content).
pub fn builtin_coil() -> Result<Definition, CmdError> {
    let r = [(-5.0, -10.0), (5.0, -10.0), (5.0, 10.0), (-5.0, 10.0)];
    let mut d = Definition {
        bridges: Vec::new(),
        name: "Two-point coil".into(),
        origin: None,
        graphics: vec![Primitive::Polyline { vertices: r.iter().map(|(x, y)| Pt::new(*x, *y)).collect(), closed: true }],
        connection_points: vec![
            ConnectionPointDef {
                key: ecad_model::PointKey::def("A1"),
                label: Some("A1".into()),
                at: Pt::new(0.0, 10.0),
                accepts: Default::default(),
            },
            ConnectionPointDef {
                key: ecad_model::PointKey::def("A2"),
                label: Some("A2".into()),
                at: Pt::new(0.0, -10.0),
                accepts: Default::default(),
            },
        ],
        ..Default::default()
    };
    let content_hash = ecad_file::definition_hash(&d).map_err(|e| CmdError::refused(format!("The built-in coil could not be prepared ({e}).")))?;
    d.origin = Some(Origin { library: BUILTIN_LIBRARY, definition: BUILTIN_COIL, library_seq: 1, content_hash });
    Ok(d)
}
