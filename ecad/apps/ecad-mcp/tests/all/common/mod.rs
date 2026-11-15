//! Helpers shared by this crate's tests (brief 062): an in-memory MCP client with writes on, and the
//! "one transaction, by the agent" check that nearly every command test makes.
//!
//! [`Client`] and [`AGENT`] stay in `mcp_contracts` (re-exported here), where the other tests import them.

#![allow(dead_code)]

use ecad_mcp::{EcadMcp, Options};
use ecad_model::{Author, Transaction};

pub(crate) use crate::mcp_contracts::{AGENT, Client};

/// A client on a fresh in-memory project made with `seed`, writes on.
pub(crate) fn writer(seed: u64) -> Client {
    Client::new(EcadMcp::in_memory("mcp", seed, Options { write: true, agent: None }).unwrap())
}

/// Exactly one transaction since the log had `before` entries, and it is by the agent. `what` names
/// the step in the failure message. Returns the transaction.
pub(crate) fn assert_one_by_agent(c: &mut Client, before: usize, what: &str) -> Transaction {
    let log = c.log();
    assert_eq!(log.len(), before + 1, "{what}: one transaction");
    let last = log.last().cloned().unwrap();
    assert_eq!(last.author, Author::Agent(AGENT.into()), "{what}: {}", last.label);
    last
}

/// [`assert_one_by_agent`], and its label is `label`.
pub(crate) fn assert_one_labelled(c: &mut Client, before: usize, label: &str) {
    let t = assert_one_by_agent(c, before, label);
    assert_eq!(t.label, label);
}
