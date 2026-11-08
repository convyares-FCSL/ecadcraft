//! ECAD-SEAM(owner): embedder commands join the registry. Its own test binary, because the
//! registry is built once per process.

use cadcraft_engine::{CommandSpec, Session};
use serde_json::{Value, json};

fn ext_specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("ecadprobe", "ECAD probe", |_s, _p| Ok(json!("probe ran"))).params("{}")]
}

#[test]
fn extension_commands_join_the_registry() {
    assert!(cadcraft_engine::cmd::set_extension_commands(ext_specs));
    assert!(cadcraft_engine::find_command("ecadprobe").is_some());
    assert!(cadcraft_engine::command_specs().iter().any(|c| c.id == "line"));
    let mut s = Session::new();
    assert_eq!(s.execute("ecadprobe", &Value::Null).unwrap(), json!("probe ran"));
    assert!(!cadcraft_engine::cmd::set_extension_commands(ext_specs), "installed once only");
}
