//! Version names and the `version` command.

use super::*;

// ---------------------------------------------------------------- versions

/// Check a new version's name (design 020 §2.5.1).
///
/// # Errors
/// In words.
pub(crate) fn check_version_name(p: &Project, name: &str) -> Result<VersionName, String> {
    let n = name.trim();
    let len = n.chars().count();
    if len == 0 || len > MAX_VERSION_NAME {
        return Err(format!("A version name is 1 to {MAX_VERSION_NAME} characters."));
    }
    if n.chars().any(char::is_control) {
        return Err("A version name cannot hold control characters.".into());
    }
    // Versions and revisions are told apart by kind: a version may be named `B` or `Rev B`.
    let lower = n.to_lowercase();
    if p.ledger.document().versions().keys().any(|k| matches!(k, PointName::Version(v) if v.as_str().to_lowercase() == lower)) {
        return Err(format!("There is already a version \"{n}\" on {}.", current_name(p)));
    }
    VersionName::new(n).map_err(|e| e.to_string())
}

/// The New version dialog's default name: `Version <n>`, n = this branch's version count + 1.
pub fn default_version_name(p: &Project) -> String {
    let own = named_points(p, current(p)).map(|v| v.iter().filter(|n| !n.is_revision() && n.branch == current(p)).count()).unwrap_or(0);
    let mut n = own + 1;
    loop {
        let name = format!("Version {n}");
        if check_version_name(p, &name).is_ok() {
            return name;
        }
        n += 1;
    }
}

/// The New branch dialog's default name: `Branch <n>`.
pub fn default_branch_name(p: &Project) -> String {
    let taken: BTreeSet<String> =
        branch_rows(p).unwrap_or_default().into_iter().filter(|(_, b)| b.status == BranchStatus::Open).map(|(_, b)| b.name.to_lowercase()).collect();
    (1..).map(|n| format!("Branch {n}")).find(|n| !taken.contains(&n.to_lowercase())).unwrap_or_else(|| "Branch".into())
}

/// `version {name, description?}`: keep the project as it is now under this name; one step.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct VersionParams {
    /// The version's name.
    name: String,
    /// What it is (at most 1 000 characters).
    #[serde(default)]
    description: Option<String>,
}

impl Command for VersionParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let description = self.description.as_deref().map(str::trim).filter(|d| !d.is_empty()).map(str::to_string);
        on_project(s, |p, pg, _| {
            let head = p.ledger.head();
            if head == 0 {
                return Err(CmdError::refused("Nothing to keep yet: draw or change something first."));
            }
            let name = check_version_name(p, &self.name).map_err(CmdError::refused)?;
            if description.as_ref().is_some_and(|d| d.chars().count() > 1_000 || d.chars().any(|c| c.is_control() && c != '\n')) {
                return Err(CmdError::refused("A description is at most 1 000 characters."));
            }
            let ops = vec![Op::NameVersion { name: PointName::Version(name.clone()), at: head, description: description.clone() }];
            p.apply_edit(pg, "version", &format!("Create version \"{name}\""), ops)?;
            let line = format!("Version \"{name}\" created. Save to keep it.");
            pg.echo(line.clone());
            Ok(json!({"name": name, "at": head, "transaction": p.ledger.head(), "message": line}))
        })
    }
}
