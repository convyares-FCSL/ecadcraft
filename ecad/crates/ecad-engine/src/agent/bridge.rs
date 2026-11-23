//! The words both ends of the live bridge share: the client in
//! `ecad-mcp` and the ECAD window that answers it.
//!
//! Excluded: the transport (the client's `Link` in `ecad-mcp`, the window's listener in the desktop app).

/// The upstream control methods the window answers over the bridge besides `ecad.*`;
/// everything else is refused there without running.
pub const ALLOWED_METHODS: &[&str] = &["drawing.inspect", "engine.commands", "ui.render", "ui.screenshot", "ping", "control.methods"];

/// Said when the window does not know the session (it restarted): the client says hello again.
pub const SAY_HELLO: &str = "say ecad.hello first";

/// Said when a method is not offered over ECAD's port (also by the window).
pub fn not_offered(method: &str) -> String {
    let why = match method {
        m if m.starts_with("cmdline.") => "ECAD changes go through execute only",
        "app.open" | "app.save" | "app.quit" => "files stay with the person",
        m if m.starts_with("ui.") => "the agent does not use the person's mouse and keyboard",
        m if m.starts_with("engine.") => "ECAD changes go through ecad.execute only",
        _ => "not one of ECAD's methods",
    };
    format!("not offered over ECAD's port: {why}")
}
