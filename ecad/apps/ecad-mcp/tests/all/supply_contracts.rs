//! A potential's supply and a motor's rated voltage through MCP: `wiresettings` with a supply and
//! `setprop rated_voltage` each record one transaction by `Author::Agent(<client>)`; the motor's
//! offer is never written by itself; a rated voltage that differs from the supply is the Checks row
//! `supply-mismatch`; with writes off both writes are refused and nothing is recorded.

use ecad_engine::agent::policy;
use ecad_mcp::{EcadMcp, Options};
use ecad_model::Author;
use serde_json::{Value, json};

use crate::mcp_contracts::{AGENT, Client, TempDir};

fn mismatches(c: &mut Client) -> Vec<String> {
    let v = c.exec("checks", json!({"kinds": ["supply-mismatch"]}));
    v["checks"].as_array().cloned().unwrap_or_default().iter().map(|r| r["which"].as_str().unwrap_or_default().to_string()).collect()
}

/// The command recorded exactly one transaction, by the agent.
fn one_by_agent(c: &mut Client, cmd: &str, params: Value) -> Value {
    let n = c.log().len();
    let v = c.exec(cmd, params.clone());
    let log = c.log();
    assert_eq!(log.len(), n + 1, "{cmd} {params}: one transaction");
    assert_eq!(log.last().map(|t| t.author.clone()), Some(Author::Agent(AGENT.into())), "{cmd}");
    v
}

fn supplied_table(c: &mut Client) -> Vec<Value> {
    let mut table = c.exec("wiresettings", json!({}))["potentials"].as_array().cloned().unwrap_or_default();
    for row in &mut table {
        if row["name"] == "L1" {
            row["supply"] = json!({"current": "ac", "nominal": 400, "phases": "three", "frequency": 50});
        }
    }
    table
}

#[test]
fn a_supply_and_a_rated_voltage_by_the_agent_then_refused_with_writes_off() {
    let dir = TempDir::new("d4 supply");
    let path = dir.file("Motor feed.ecad");
    let (motor, table) = {
        let mut c = Client::new(EcadMcp::create(&path, Options { write: true, agent: None }).unwrap());
        let table = supplied_table(&mut c);
        one_by_agent(&mut c, "wiresettings", json!({"potentials": table.clone()}));
        let m = c.exec("place", json!({"symbol": "motor3", "at": [100, 100]}))["component"].clone();
        let doc = c.doc();
        let id = ecad_model::DeviceId::parse(m.as_str().unwrap()).unwrap();
        let at = ecad_model::connectivity::derive::connection_point_positions(&doc, doc.page_order()[0])
            .into_iter()
            .find(|(t, _, _)| doc.connection_points().get(t).and_then(|x| x.device()) == Some(id))
            .map(|(_, _, p)| p)
            .unwrap();
        // A wire runs between two connection points: one at its free end.
        c.points_at(&[(at.x, at.y + 40.0)]);
        c.exec("wire", json!({"points": [[at.x, at.y], [at.x, at.y + 40.0]], "potential": "L1"}));
        // Offered, never written: the motor has no rated voltage and no row.
        assert!(mismatches(&mut c).is_empty());
        let rated = |c: &mut Client| match &c.doc().devices()[&id].device_type {
            ecad_model::DeviceType::Motor(x) => x.rated_voltage,
            _ => None,
        };
        assert_eq!(rated(&mut c), None);
        one_by_agent(&mut c, "setprop", json!({"component": m, "key": "rated_voltage", "value": "230 V"}));
        let rows = mismatches(&mut c);
        assert!(rows.len() == 1 && rows[0].contains("its supply gives 400 V line-to-line"), "{rows:?}");
        assert_eq!(c.ok("save", json!({}))["saved"], json!(true));
        (m, table)
    };
    // Writes off: both writes refused, nothing recorded.
    let (sess, _) = EcadMcp::open(&path, Options::default()).unwrap();
    let mut c = Client::new(sess);
    assert!(!c.s.writes());
    let log = c.log();
    for (cmd, params) in
        [("setprop", json!({"component": motor, "key": "rated_voltage", "value": "400 V"})), ("wiresettings", json!({"potentials": table}))]
    {
        let m = c.refused("execute", json!({"command": cmd, "params": params}));
        assert_eq!(m, policy::WRITES_OFF, "{cmd} {params}");
    }
    assert_eq!(c.log(), log, "nothing recorded with writes off");
    assert_eq!(mismatches(&mut c).len(), 1, "the saved rating and supply read back");
}
