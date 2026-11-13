//! Brief 043 (U4d): History's session groups ([`history::sessions`]), a read helper over the
//! log: consecutive transactions by one agent form one group; the person's are one row each.

use ecad_cadcraft::history::{self, SessionGroup};
use ecad_model::{Author, Transaction, TxId, TxKind};

fn tx(seq: u64, author: &Author, minute: u64) -> Transaction {
    Transaction {
        id: TxId(u128::from(seq)),
        seq,
        author: author.clone(),
        label: format!("step {seq}"),
        kind: TxKind::Edit,
        time_ms: Some(1_700_000_000_000 + minute * 60_000),
        ops: Vec::new(),
    }
}

/// 3 person edits, 14 agent transactions, 2 person edits, newest first as the Steps list reads.
fn log() -> Vec<Transaction> {
    let you = Author::User;
    let claude = Author::Agent("claude".into());
    let mut v: Vec<Transaction> = (1..=3).map(|s| tx(s, &you, s)).collect();
    v.extend((4..=17).map(|s| tx(s, &claude, s)));
    v.extend((18..=19).map(|s| tx(s, &you, s)));
    v.reverse();
    v
}

#[test]
fn an_agents_consecutive_transactions_are_one_group_and_the_persons_one_row_each() {
    let groups = history::sessions(&log());
    let sizes: Vec<(bool, usize)> = groups.iter().map(|g| (g.is_session(), g.seqs.len())).collect();
    assert_eq!(sizes, [(false, 1), (false, 1), (true, 14), (false, 1), (false, 1), (false, 1)]);
    let g = &groups[2];
    assert_eq!(g.author, Author::Agent("claude".into()));
    assert_eq!(g.seqs.first(), Some(&17), "the order given (newest first)");
    assert_eq!(g.ids.len(), 14);
    // Minutes 4 and 17 after 22:13 UTC.
    assert_eq!(g.text_in(Some(0)), "claude \u{b7} 14 changes \u{b7} 22:17\u{2013}22:30");
    assert_eq!(g.text_in(None), "claude \u{b7} 14 changes \u{b7} 22:17\u{2013}22:30 UTC");
}

#[test]
fn two_agents_or_a_lone_agent_step_are_not_one_session() {
    let a = Author::Agent("a".into());
    let b = Author::Agent("b".into());
    let groups = history::sessions(&[tx(3, &a, 3), tx(2, &b, 2), tx(1, &b, 1)]);
    assert_eq!(groups.iter().map(|g| g.seqs.len()).collect::<Vec<_>>(), [1, 2]);
    assert!(!groups[0].is_session(), "one agent step alone is a plain row");
    assert!(groups[1].is_session());
    assert_eq!(history::sessions(&[]), Vec::<SessionGroup>::new());
    // No time known: no span.
    let mut t = tx(1, &a, 0);
    t.time_ms = None;
    assert_eq!(history::sessions(&[t])[0].text_in(Some(0)), "a \u{b7} 1 change");
}

#[test]
fn two_sessions_of_one_agent_back_to_back_split_where_the_marked_session_starts() {
    let claude = Author::Agent("claude".into());
    // Newest first: the live session's 3 steps (seq 13..=11) on top of an ended one's 10.
    let log: Vec<Transaction> = (1..=13).rev().map(|s| tx(s, &claude, s)).collect();
    let live = |id: &TxId| id.0 >= 11;
    let groups = history::sessions_split(&log, live);
    assert_eq!(groups.iter().map(|g| g.seqs.len()).collect::<Vec<_>>(), [3, 10]);
    assert_eq!(history::sessions(&log).len(), 1, "unmarked, the name alone joins them");
}
