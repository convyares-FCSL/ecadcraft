//! Helpers shared by this crate's tests: an in-memory MCP client with writes on, and the
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

/// A loopback port nothing listens on now, for a test's window or server to bind.
///
/// Ports come from 20000–31999, below the ranges Windows (49152–65535) and Linux (32768–60999)
/// give to outgoing connections and to binds on port 0, so another test's connection cannot take
/// the port between this check and the bind. A port is handed out once per test process, and the
/// first one is spread by process id, so test processes run side by side start apart.
pub(crate) fn free_port() -> u16 {
    use std::sync::atomic::{AtomicU32, Ordering};
    const LO: u32 = 20_000;
    const SPAN: u32 = 12_000;
    static NEXT: AtomicU32 = AtomicU32::new(u32::MAX);
    let first = std::process::id().wrapping_mul(613) % SPAN;
    let _ = NEXT.compare_exchange(u32::MAX, first, Ordering::Relaxed, Ordering::Relaxed);
    for _ in 0..SPAN {
        let port = u16::try_from(LO + NEXT.fetch_add(1, Ordering::Relaxed) % SPAN).unwrap();
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    panic!("no free loopback port in 20000–31999");
}
