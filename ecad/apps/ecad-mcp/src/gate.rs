//! The write gate (brief 011, moved here by brief 035): the policy check, the per-call
//! transaction cap and its roll-back, the agent-undo rule and the agent's author, on whatever
//! session and project the caller passes.
//!
//! Two hosts use it: the headless server ([`crate::EcadMcp`], one session of its own) and the
//! trial's live bridge (the person's own session, one call between two frames). The same project
//! state gives the same reply text in both.
//!
//! # Semantics
//! - [`execute`] runs one command through `Session::execute` (never `CadApp::run`, so UI-only
//!   commands stay unknown) after [`crate::policy::check`], with the project told how many
//!   transactions the call may still record; a call that records more is taken back and refused.
//! - [`Call::refusal`] replaces [`crate::policy::WRITES_OFF`] when the host refuses writes for a
//!   reason of its own (the trial's Read only, Pause, another writer).
//! - With [`Call::owned`] (the bridge), `u` and `redo` act only on the session's own steps, and
//!   every transaction the call keeps is added to them.
//! - [`as_agent`] records the agent as the author for one call and restores the person after it,
//!   also when the call fails or panics.
//!
//! Excluded: anything egui, the transport, and what a host shows.

use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

use cadcraft_engine::Session;
use ecad_cadcraft::ProjectHandle;
use ecad_model::{Author, Transaction, TxId, TxKind};
use serde_json::{Value, json};

use crate::{MAX_TRANSACTIONS_PER_CALL, UNREADABLE, policy};

/// The transactions one bridge session recorded (its own steps, for the agent-undo rule and
/// Undo session), oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnedSteps {
    ids: BTreeSet<TxId>,
    order: Vec<TxId>,
}

/// Most steps remembered per bridge session; older ones are forgotten (the agent cannot undo
/// them any more, which only narrows what it may do).
pub const MAX_OWNED_STEPS: usize = 100_000;

impl OwnedSteps {
    /// Remember `id` as the session's.
    pub fn add(&mut self, id: TxId) {
        if self.ids.insert(id) {
            self.order.push(id);
            if self.order.len() > MAX_OWNED_STEPS {
                let old = self.order.remove(0);
                self.ids.remove(&old);
            }
        }
    }

    /// Whether `id` is one of the session's.
    pub fn contains(&self, id: &TxId) -> bool {
        self.ids.contains(id)
    }

    /// How many the session recorded.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Whether the session recorded none.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// The session's transactions, oldest first.
    pub fn ids(&self) -> &[TxId] {
        &self.order
    }
}

/// One call through the gate.
pub struct Call<'a> {
    /// Whether writes are allowed for this call (`--write`, and on the bridge the window's own
    /// conditions).
    pub write: bool,
    /// Said instead of [`policy::WRITES_OFF`] when `write` is off for a reason of the host's.
    pub refusal: Option<&'a str>,
    /// Transactions already recorded by the request line this call belongs to (one request line
    /// is one call for [`MAX_TRANSACTIONS_PER_CALL`], batches included). Updated by the call.
    pub line_transactions: &'a mut usize,
    /// The bridge session's own steps: `u`/`redo` are limited to them and the call's transactions
    /// are added. `None` headless (only the agent edits there).
    pub owned: Option<&'a mut OwnedSteps>,
}

impl Call<'_> {
    fn writes_off(&self) -> String {
        self.refusal.map_or_else(|| policy::WRITES_OFF.to_string(), str::to_string)
    }
}

/// Why the project cannot be written, if it cannot (damaged, or another session edits it).
///
/// # Errors
/// [`UNREADABLE`] when the project cannot be read (callers refuse: fail closed).
pub fn read_only(s: &mut Session, p: &ProjectHandle) -> Result<Option<String>, String> {
    p.read(s, |p, _| p.file_status().read_only).map_err(|_| UNREADABLE.to_string())
}

/// Refuse a write when writes are off for `call`, or the project is read-only.
///
/// # Errors
/// The refusal in words.
pub fn may_write(s: &mut Session, p: &ProjectHandle, call: &Call<'_>) -> Result<(), String> {
    if !call.write {
        return Err(call.writes_off());
    }
    match read_only(s, p)? {
        Some(why) => Err(policy::project_read_only(&why)),
        None => Ok(()),
    }
}

/// The branch shown and the length of its log in memory: where a call's transactions start.
fn log_len(s: &mut Session, p: &ProjectHandle) -> Result<(String, usize), String> {
    p.read(s, |p, _| (ecad_cadcraft::history::current(p).hex(), p.ledger.log().len())).map_err(|_| UNREADABLE.to_string())
}

/// The transactions recorded since `from`. A call that switched branch (brief 020) saved the
/// branch it left first and switching is not a step, so what it recorded is the new branch's
/// unsaved steps (a merge into the parent switches, then records one merge: brief 020b).
fn log_since(s: &mut Session, p: &ProjectHandle, from: &(String, usize)) -> Result<Vec<Transaction>, String> {
    p.read(s, |p, _| {
        if ecad_cadcraft::history::current(p).hex() != from.0 {
            let saved = p.ledger.persisted();
            return p.ledger.log().iter().filter(|t| t.seq > saved).cloned().collect();
        }
        p.ledger.log().get(from.1..).map(<[Transaction]>::to_vec).unwrap_or_default()
    })
}

/// Make a page of the project the active document, as the trial does before saving.
///
/// # Errors
/// No ECAD page is open, or switching failed.
pub fn focus_project(s: &mut Session) -> Result<(), String> {
    let owned = |s: &Session, uid: u64| s.ext.owner_of(uid).is_some();
    if s.state().is_ok_and(|st| owned(s, st.uid)) {
        return Ok(());
    }
    let i = s.docs.iter().position(|d| owned(s, d.uid)).ok_or("no ECAD page is open")?;
    s.execute("document.switch", &json!({ "index": i })).map(|_| ()).map_err(|e| e.to_string())
}

/// The command that takes back `t`: an undo is redone, a merge or an issue is undone with its
/// confirming form, anything else is undone.
pub fn undo_step_for(t: &Transaction) -> &'static str {
    match t.kind {
        TxKind::Undo { .. } => "redo",
        // Undoing a merge asks first; `undomerge` is that confirmation (reviewer 020b, 5).
        TxKind::Merge { .. } => "undomerge",
        TxKind::Redo { .. } if t.label.starts_with("Merge \"") || t.label.starts_with("Update from ") => "undomerge",
        // Undoing an issue asks first too; `undoissue` confirms it (tester 026, 1).
        TxKind::Edit | TxKind::Redo { .. } if ecad_cadcraft::revisions::issue_in(&t.ops).is_some() => "undoissue",
        TxKind::Edit | TxKind::Redo { .. } => "u",
    }
}

/// Take back `txs` (oldest first), newest first. Returns how many were taken back, or how many
/// were before the first failure and that failure.
///
/// # Errors
/// The count taken back before the failure, and the failure in words.
pub fn roll_back(s: &mut Session, txs: &[Transaction]) -> Result<usize, (usize, String)> {
    for (done, t) in txs.iter().rev().enumerate() {
        let step = undo_step_for(t);
        if let Err(e) = s.execute(step, &json!({})) {
            return Err((done, format!("{} failed: {e}", step.to_ascii_uppercase())));
        }
    }
    Ok(txs.len())
}

/// Tell the project how many transactions the call about to run may record (`None` after it).
fn set_budget(s: &mut Session, p: &ProjectHandle, budget: Option<usize>) -> Result<(), String> {
    p.configure(s, |p| p.tx_budget = budget)
}

/// Whether `txs` is exactly a Finalise edit followed by an issue (brief 026, design §5.5).
fn finalised_then_issued(txs: &[Transaction]) -> bool {
    match txs {
        [f, i] => {
            matches!(f.kind, TxKind::Edit)
                && f.label == "Finalise numbering"
                && matches!(i.kind, TxKind::Edit)
                && ecad_cadcraft::revisions::issue_in(&i.ops).is_some()
        }
        _ => false,
    }
}

/// Said when an agent on the bridge would undo or redo a step that is not its own.
pub fn not_own_step(seq: u64, label: &str) -> String {
    format!("Refused: the last step (#{seq} \"{label}\") is the designer's; an agent undoes only its own steps.")
}

/// The agent-undo rule (design 035 §8.1): `u` (and its confirming forms) only when the step it
/// would take back is the session's; `redo` only when the undo it would redo is.
fn check_own_step(s: &mut Session, p: &ProjectHandle, id: &str, owned: &OwnedSteps) -> Result<(), String> {
    let undo = matches!(id, "undo" | "u" | "undomerge" | "undoissue");
    let redo = matches!(id, "redo" | "mredo");
    if !undo && !redo {
        return Ok(());
    }
    let top = p
        .read(s, |p, _| {
            let t = if undo { p.ledger.undo_top() } else { p.ledger.redo_top() };
            t.map(|t| (t.id, t.seq, t.label.clone()))
        })
        .map_err(|_| UNREADABLE.to_string())?;
    match top {
        // Nothing to undo or redo: the engine says so in its own words.
        None => Ok(()),
        Some((tx, _, _)) if owned.contains(&tx) => Ok(()),
        Some((_, seq, label)) => Err(not_own_step(seq, &label)),
    }
}

/// A transaction as the tools report it.
pub fn tx_summary(t: &Transaction) -> Value {
    json!({
        "seq": t.seq,
        "id": t.id.hex(),
        "label": t.label,
        "author": crate::author_text(&t.author),
        "ops": t.ops.len(),
    })
}

/// `engine.execute` through the gate (`{command, params}`), with the caps and the transactions it
/// recorded (`ecadTransactions`).
///
/// # Errors
/// The refusal or the command's error, in words.
pub fn execute(s: &mut Session, p: &ProjectHandle, call: &mut Call<'_>, req: &Value) -> Result<Value, String> {
    let given = req.get("command").and_then(Value::as_str).ok_or("missing `command`")?.to_string();
    let params = req.get("params").cloned().filter(|v| !v.is_null()).unwrap_or_else(|| json!({}));
    let id = cadcraft_engine::find_command(&given).map_or_else(|| given.to_ascii_lowercase(), |c| c.id.to_string());
    let ro = read_only(s, p)?;
    let writes_off = call.writes_off();
    policy::check(&id, &params, call.write, ro.as_deref()).map_err(|e| if e == policy::WRITES_OFF { writes_off } else { e })?;
    if let Some(owned) = call.owned.as_deref() {
        check_own_step(s, p, &id, owned)?;
    }
    // Fail closed: a call whose transactions cannot be counted does not run.
    let before = log_len(s, p)?;
    // What this call may still record, told to the project before it runs: a command that
    // writes files (brief 027's release) refuses up front rather than write what the cap
    // below would take back (tester 027, 3).
    let line_used = *call.line_transactions;
    let cap = if line_used == 0 { policy::transactions_allowed(&id, &params) } else { MAX_TRANSACTIONS_PER_CALL };
    set_budget(s, p, Some(cap.saturating_sub(line_used)))?;
    let r = s.execute(&given, &params).map_err(|e| e.to_string());
    set_budget(s, p, None)?;
    let name = id.to_ascii_uppercase();
    let new = log_since(s, p, &before).map_err(|e| format!("{name} ran, but its transactions could not be read back ({e}); check ecad_history."))?;
    let used = *call.line_transactions;
    // Brief 026 (design §5.5): `issuerevision {provisional: "finalise"}` records Finalise, then
    // the issue: two transactions by design (the revision point holds the finalised tags).
    // Only when this call is the line's first write and it really recorded Finalise, then the
    // issue (reviewer 026, 3); anything else keeps the cap of one.
    let allowed = if used == 0 && finalised_then_issued(&new) { policy::transactions_allowed(&id, &params) } else { MAX_TRANSACTIONS_PER_CALL };
    if used.saturating_add(new.len()) > allowed {
        let what = if used > 0 {
            format!("this request line had already recorded {used}, and {name} recorded {}", new.len())
        } else {
            format!("{name} recorded {}", new.len())
        };
        let seqs: Vec<String> = new.iter().map(|t| format!("#{}", t.seq)).collect();
        let seqs = seqs.join(", ");
        return Err(match roll_back(s, &new) {
            Ok(n) => format!(
                "Refused: one call (one request line, batches included) may record at most \
                 {allowed} transaction(s); {what} ({seqs}). They were taken back by {n} \
                 undo/redo transaction(s) recorded after them, so the design is as before {name}; the history \
                 keeps both."
            ),
            Err((done, e)) => format!(
                "Refused: one call (one request line, batches included) may record at most \
                 {allowed} transaction(s); {what} ({seqs}). Taking them back stopped after {done} \
                 of {}: {e}. The project is NOT as before {name}; check ecad_history.",
                new.len()
            ),
        });
    }
    *call.line_transactions = used.saturating_add(new.len());
    if let Some(owned) = call.owned.as_deref_mut() {
        for t in &new {
            owned.add(t.id);
        }
    }
    let v = r?;
    if new.is_empty() {
        return Ok(v);
    }
    let txs: Vec<Value> = new.iter().map(tx_summary).collect();
    Ok(match v {
        Value::Object(mut o) => {
            o.insert("ecadTransactions".into(), Value::Array(txs));
            Value::Object(o)
        }
        other => json!({ "result": other, "ecadTransactions": txs }),
    })
}

/// What [`as_agent`] found.
#[derive(Debug)]
pub struct Scoped<R> {
    /// The call's result; `None` when it panicked (the panic was caught).
    pub result: Option<R>,
    /// Whether the person was recorded as the author again after the call. `false` (the project
    /// stayed busy through every attempt) means the host must keep trying before the person's next
    /// edit, and refuse agent calls until it has.
    pub restored: bool,
}

/// Run `f` with `Author::Agent(name)` as the author of the project's transactions, then restore
/// `Author::User` and no transaction budget, also when `f` returns an error or panics.
///
/// # Errors
/// The author could not be set (the project is busy or not attached): nothing ran (fail closed,
/// [`UNREADABLE`]).
pub fn as_agent<R>(s: &mut Session, p: &ProjectHandle, name: &str, f: impl FnOnce(&mut Session) -> R) -> Result<Scoped<R>, String> {
    p.configure(s, |p| p.author = Author::Agent(name.to_string())).map_err(|_| UNREADABLE.to_string())?;
    let result = catch_unwind(AssertUnwindSafe(|| f(s))).ok();
    let restored = restore_user_retrying(s, p);
    Ok(Scoped { result, restored })
}

/// Record the person as the author again and clear any budget. Returns whether it worked.
pub fn restore_user(s: &mut Session, p: &ProjectHandle) -> bool {
    p.configure(s, |p| {
        p.author = Author::User;
        p.tx_budget = None;
    })
    .is_ok()
}

/// How many times [`restore_user_retrying`] tries.
pub const RESTORE_ATTEMPTS: usize = 3;

/// [`restore_user`], tried up to [`RESTORE_ATTEMPTS`] times: the project lock can be held for a
/// moment (the read-only debugger copies under `try_lock`), and a busy lock must not leave the
/// agent as the author.
pub fn restore_user_retrying(s: &mut Session, p: &ProjectHandle) -> bool {
    for _ in 0..RESTORE_ATTEMPTS {
        if restore_user(s, p) {
            return true;
        }
        std::thread::yield_now();
    }
    false
}

/// Whether the project records the person as the author with no budget (the trial checks it at
/// the top of every frame). `None` when the project cannot be read just now.
pub fn author_is_user(s: &mut Session, p: &ProjectHandle) -> Option<bool> {
    p.read(s, |p, _| p.author == Author::User && p.tx_budget.is_none()).ok()
}
