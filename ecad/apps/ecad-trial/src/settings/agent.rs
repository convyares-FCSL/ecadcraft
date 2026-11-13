//! Settings ▸ Agent access (brief 035's row, design 035 §3.2: "until the UI shell gives settings
//! a home"; U0 O7): Off, Read only or Read and change, the port, and what listens. 035's element
//! names are kept (`agent.access.mode`, `agent.access.port`, `agent.access.status`).

use eframe::egui;

use crate::Trial;
use crate::developer;

/// Draw the section.
pub fn show(t: &mut Trial, ui: &mut egui::Ui) {
    ecad_ui::prim::read_value(ui, "Whether an agent (through ECAD's MCP bridge) may read this project, or read and change it.");
    ui.add_space(ecad_ui::tokens::space::S);
    let busy = t.cad.session.running.is_some();
    let mut out = Vec::new();
    developer::agent_row(
        ui,
        developer::AgentRow {
            mode: t.agent.pref.mode,
            status: t.agent.status_text(),
            warning: t.agent.author_pending || matches!(t.agent.listen(), crate::agent::Listen::InUse(_)),
            locked: t.agent.this_run,
            port_warning: t.agent.port_warning,
            restart_note: t.agent.restart_note,
            // The banner has its own slot under the top bar.
            banner: None,
            busy,
            port: &mut t.agent.port_text,
        },
        &mut out,
    );
    for a in out {
        t.developer_action(a);
    }
}
