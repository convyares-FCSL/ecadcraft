//! Lane R11 (design 061, rayon): the debugger's checks run on the background pool, on copies, and a
//! result computed before a commit landed is dropped, never applied.

use std::time::Duration;

use ecad_app::App;
use ecad_app::developer::debugger::{Action, CheckKind};

const WAIT: Duration = Duration::from_secs(30);

fn new_app() -> App {
    ecad_engine::install();
    ecad_engine::cables::views::isolate_for_tests();
    App::new(None).unwrap()
}

fn ok(t: &mut App, line: &str) {
    let e = t.submit_command(line);
    assert!(e.is_none(), "{line}: {e:?}");
}

fn frame(t: &mut App, ctx: &eframe::egui::Context) {
    let mut out = ctx.run_ui(eframe::egui::RawInput::default(), |ui| {
        t.logic(ui.ctx());
        t.ui(ui);
    });
    out.textures_delta.clear();
}

#[test]
fn a_commit_while_a_check_runs_drops_the_old_result() {
    let mut t = new_app();
    let ctx = eframe::egui::Context::default();
    ok(&mut t, r#"place {"at":[100,0],"rotation":90}"#);
    t.toggle_debugger();
    frame(&mut t, &ctx);
    let before = t.debugger.snapshot().unwrap().head;

    // Every check starts at `before`; a commit lands before any result is taken.
    for k in [CheckKind::Replay, CheckKind::Projection, CheckKind::Index] {
        t.debugger_action(Action::Check(k));
    }
    assert!(t.debugger.checks().is_empty(), "nothing is applied in the frame that started them");
    ok(&mut t, r#"place {"at":[150,0],"rotation":0}"#);
    assert!(t.debugger.wait_for_checks(WAIT), "the checks finish");
    frame(&mut t, &ctx);
    assert_eq!(t.debugger.snapshot().unwrap().head, before + 1);
    assert!(t.debugger.checks().is_empty(), "results at the old head are not applied: {:?}", t.debugger.checks());
    assert_eq!(t.debugger.dropped_checks(), 3);
    assert_eq!(t.debugger.running_checks(), 0);

    // Started at the new head, the same checks are taken on the next frame.
    for k in [CheckKind::Replay, CheckKind::Projection, CheckKind::Index] {
        t.debugger_action(Action::Check(k));
    }
    assert!(t.debugger.wait_for_checks(WAIT));
    frame(&mut t, &ctx);
    let ran = t.debugger.checks();
    assert_eq!(ran.len(), 3);
    assert!(ran.iter().all(|r| r.check.ok && r.check.at_seq == before + 1), "{ran:?}");
    assert_eq!(t.debugger.dropped_checks(), 3, "nothing more dropped");
}
