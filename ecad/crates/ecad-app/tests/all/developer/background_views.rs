//! Issue 48 (lane D2): the Checks list and the report models are first built on the background
//! pool for the Checks tab, the Reports window and the pre-release check; each says "Running…"
//! meanwhile, and a result computed before a commit landed is never applied.

use std::time::Duration;

use ecad_app::App;

const WAIT: Duration = Duration::from_secs(30);

fn new_app() -> App {
    ecad_engine::install();
    ecad_engine::cables::views::isolate_for_tests();
    let mut t = App::new(None).unwrap();
    t.warm.set_inline(false);
    t
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

/// The shared steps: `shown` says whether the surface still waits; a commit lands while the builds
/// run, the old results are dropped, and the next state's builds are applied one call later.
fn drive(t: &mut App, refresh: &mut dyn FnMut(&mut App), waiting: &dyn Fn(&mut App) -> bool) {
    refresh(t);
    assert!(waiting(t), "the first build is running, not done");
    ok(t, r#"place {"at":[100,0],"rotation":90}"#);
    assert!(t.warm.wait(WAIT));
    refresh(t);
    assert!(waiting(t), "the result of the old head is not applied");
    assert_eq!(t.warm.dropped(), 2, "both builds were dropped");
    assert!(t.warm.wait(WAIT));
    refresh(t);
    assert!(!waiting(t), "the builds started at the new head are applied");
    assert_eq!(t.warm.dropped(), 2, "nothing more dropped");
}

#[test]
fn the_checks_tab_waits_for_its_first_build_and_drops_a_result_a_commit_overtook() {
    let mut t = new_app();
    ok(&mut t, r#"place {"at":[50,0],"rotation":0}"#);
    drive(&mut t, &mut |t| t.refresh_panel(), &|t| t.panel.checks.building);
    assert!(t.panel.checks.copies >= 1, "the rows were copied once built");
}

#[test]
fn the_reports_window_waits_for_its_first_build_and_drops_a_result_a_commit_overtook() {
    let mut t = new_app();
    let ctx = eframe::egui::Context::default();
    ok(&mut t, r#"place {"at":[50,0],"rotation":0}"#);
    t.open_reports();
    let waiting = |t: &App| t.reports.as_ref().is_some_and(|r| r.building);
    // The Checks tab's refresh starts the builds (a frame would also refresh the Reports window
    // at once, and a build of this small project can already be done by then).
    t.refresh_panel();
    assert_eq!(t.warm.running(), 2, "the builds are running, nothing is applied");
    ok(&mut t, r#"place {"at":[100,0],"rotation":90}"#);
    assert!(t.warm.wait(WAIT));
    // Two refreshes in one frame (Checks tab, Reports window): the second may already see the
    // restarted builds done, so only what was dropped is asserted here.
    frame(&mut t, &ctx);
    assert_eq!(t.warm.dropped(), 2, "both builds of the old head were dropped, not applied");
    assert!(t.warm.wait(WAIT));
    frame(&mut t, &ctx);
    assert!(!waiting(&t), "the builds started at the new head are applied");
    assert_eq!(t.warm.dropped(), 2, "nothing more dropped");
}

#[test]
fn the_pre_release_check_waits_for_its_first_build_and_drops_a_result_a_commit_overtook() {
    let mut t = new_app();
    ok(&mut t, r#"place {"at":[50,0],"rotation":0}"#);
    t.open_release(None);
    drive(&mut t, &mut |t| t.refresh_release(), &|t| t.release.as_ref().is_some_and(|d| d.view.is_none()));
}
