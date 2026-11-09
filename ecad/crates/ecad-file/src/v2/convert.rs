//! The version 1 → version 2 converter (EEP-0005 §10, EEP-0003 §16.2).
//!
//! - The source is only read: it is never locked for writing, modified, truncated or renamed.
//! - The output is a new file, written by the Save As steps (temporary file, rename, folder
//!   flush); an existing file is never overwritten.
//! - Only a sound source converts: a clean one, or one with an interrupted trailing write (its
//!   verified prefix converts and the report names the omitted bytes). A corrupt source is
//!   refused with its open report.
//! - The whole history becomes `main`, transaction for transaction, with the version 1 records
//!   stored byte for byte (so the chain values are version 1's), and `meta.link_v1` records the
//!   source's final hash and length. Named versions become `versions` rows with their snapshot.
//! - The result is verified by replaying both and comparing the documents, every transaction and
//!   every version's snapshot; any difference deletes the new file and fails loudly.

use std::path::{Path, PathBuf};

use ecad_model::Ledger;
use serde_json::Value;

use super::project::ProjectV2;
use super::schema::{DEFAULT_SNAPSHOT_EVERY, LinkV1, VersionKind};
use super::store::Store;
use crate::chain;
use crate::codec::{Codec, JsonLines};
use crate::read::{self, Outcome};

/// What a conversion did (EEP-0003 §16.2 item 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertReport {
    pub source: PathBuf,
    pub output: PathBuf,
    pub source_container: u64,
    pub source_schema: u64,
    pub transactions: u64,
    /// The source's final chain hash (hex).
    pub source_hash: String,
    /// The new file's head chain value (hex): equal to `source_hash`, as the records are kept.
    pub head: String,
    pub bytes_before: u64,
    pub bytes_after: u64,
    /// Bytes of an interrupted trailing write that were not converted.
    pub omitted: Option<String>,
    pub named_versions: usize,
    pub verified: bool,
}

impl ConvertReport {
    /// A few lines for the user.
    pub fn describe(&self) -> String {
        let mut s = format!(
            "Converted {} (container {}, schema {}) to {} (container 2, codec json/1): {} transactions, \
             {} named versions, {} bytes -> {} bytes. Final hash {}, carried over unchanged. Verified by replaying both.",
            self.source.display(),
            self.source_container,
            self.source_schema,
            self.output.display(),
            self.transactions,
            self.named_versions,
            self.bytes_before,
            self.bytes_after,
            self.source_hash
        );
        if let Some(o) = &self.omitted {
            s.push_str(&format!(" Not converted: {o}."));
        }
        s
    }
}

/// The output a conversion writes by default: `<name>.v2.ecad` beside the source.
pub fn default_output(source: &Path) -> PathBuf {
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "project".into());
    source.with_file_name(format!("{stem}.v2.ecad"))
}

/// Which major a file is, from its first bytes (E3 §16.1 item 3, adapted to redb).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Major {
    /// JSON Lines: version 1.
    V1,
    /// A redb database: version 2.
    V2,
    Unknown,
}

/// redb's file magic.
const REDB_MAGIC: &[u8] = b"redb";

/// The major of the file at `path`.
///
/// # Errors
/// The file cannot be read.
pub fn sniff(path: &Path) -> std::io::Result<Major> {
    use std::io::Read;
    let mut head = [0u8; 9];
    let mut f = std::fs::File::open(path)?;
    let n = f.read(&mut head)?;
    Ok(sniff_bytes(head.get(..n).unwrap_or_default()))
}

/// The major of a file starting with `head`.
pub fn sniff_bytes(head: &[u8]) -> Major {
    match head.first() {
        Some(b'{') => Major::V1,
        _ if head.starts_with(REDB_MAGIC) => Major::V2,
        _ => Major::Unknown,
    }
}

/// The canonical bytes of the first `n` transaction records of a version 1 file (the stored
/// form the chain hashes).
fn records(bytes: &[u8], n: usize) -> Result<Vec<Vec<u8>>, String> {
    let codec = JsonLines;
    let mut out = Vec::with_capacity(n);
    for line in bytes.split(|b| *b == b'\n').skip(1).take(n) {
        let payload = codec.payload(line).map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_slice(payload).map_err(|e| e.to_string())?;
        out.push(crate::canonical::canonical(&value).map_err(|e| e.to_string())?);
    }
    if out.len() != n {
        return Err(format!("expected {n} records, found {}", out.len()));
    }
    Ok(out)
}

/// Convert the version 1 file at `source` into a new version 2 file at `output` (default
/// [`default_output`]).
///
/// # Errors
/// A message naming why: the source cannot be read or is not a sound version 1 project, the
/// output exists, writing failed, or verification found a difference (the new file is then
/// deleted). The source is never changed.
pub fn convert_file(source: &Path, output: Option<&Path>, writer: &str) -> Result<ConvertReport, String> {
    let output = output.map(Path::to_path_buf).unwrap_or_else(|| default_output(source));
    if crate::v2::disk::same_file(source, &output) {
        return Err("the output must be a new file, not the source".into());
    }
    if output.exists() {
        return Err(format!("{} already exists; it is never overwritten", output.display()));
    }
    let bytes = std::fs::read(source).map_err(|e| format!("could not read {}: {e}", source.display()))?;
    if sniff_bytes(&bytes) != Major::V1 {
        return Err(format!("{} is not a version 1 project file", source.display()));
    }
    let opened = read::open_project(&bytes, 0).map_err(|e| e.to_string())?;
    let omitted = match &opened.report.outcome {
        Outcome::Clean => None,
        Outcome::IncompleteTail { offset, len, .. } => {
            Some(format!("an interrupted trailing write of {len} bytes at offset {offset}"))
        }
        Outcome::Corrupt(_) => {
            return Err(format!(
                "the source is damaged, so it is not converted: {} Rescue it with Save As in a build that \
                 writes version 1 first.",
                opened.report.describe()
            ));
        }
    };
    let project = opened.header.project_id.ok_or("the source has no project ID")?;
    let n = opened.ledger.log().len();
    let recs = records(&bytes, n)?;
    let link = LinkV1 {
        hash: chain::hex(&opened.chain_head),
        transactions: n as u64,
        source_container: opened.header.container_version,
        source_schema: opened.header.schema_version,
    };
    // The output is created with create-new semantics (`Store::create`): never a check and then
    // a rename, so a file that appears at that name meanwhile is never replaced. The whole project
    // is one write transaction: a crash leaves an empty database there, which opens as "not an
    // ECAD project" and is never mistaken for a conversion.
    let mut created = false;
    let written = (|| -> Result<(), String> {
        let store = Store::create(&output).map_err(|e| e.to_string())?;
        created = true;
        let (p, _) = ProjectV2::create(
            store,
            project,
            &opened.ledger,
            Some(&recs),
            Some(link.clone()),
            opened.header.created_ms,
            writer,
            DEFAULT_SNAPSHOT_EVERY,
        )
        .map_err(|e| e.to_string())?;
        drop(p);
        ecad_store::disk::sync_dir(&ecad_store::disk::parent_dir(&output)).map_err(|e| e.to_string())
    })();
    if let Err(e) = written {
        if created {
            let _ = ecad_store::disk::remove_file(&output);
        }
        return Err(format!("the conversion was not written: {e}"));
    }
    match verify(&output, &opened.ledger, &link) {
        Ok(named_versions) => Ok(ConvertReport {
            source: source.to_path_buf(),
            output: output.clone(),
            source_container: opened.header.container_version,
            source_schema: opened.header.schema_version,
            transactions: n as u64,
            source_hash: link.hash.clone(),
            head: link.hash,
            bytes_before: bytes.len() as u64,
            bytes_after: std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0),
            omitted,
            named_versions,
            verified: true,
        }),
        Err(e) => {
            let _ = ecad_store::disk::remove_file(&output);
            Err(format!("the converted file did not verify, so it was deleted and nothing else changed: {e}"))
        }
    }
}

/// Replay the new file and compare it with the source's replay: the document, every
/// transaction, every version's snapshot, and the head's chain value. The number of named
/// versions.
fn verify(output: &Path, source: &Ledger, link: &LinkV1) -> Result<usize, String> {
    let store = Store::open(output, false).map_err(|e| e.to_string())?;
    let o = ProjectV2::open(store, 0, "verify").map_err(|e| e.to_string())?;
    if let Some(d) = &o.report.damage {
        return Err(format!("opening it found damage: {}", d.detail));
    }
    if o.ledger.head() != source.head() {
        return Err(format!("it has {} transactions, the source {}", o.ledger.head(), source.head()));
    }
    if chain::hex(&o.project.head().1) != link.hash {
        return Err("its head chain value differs from the source's final hash".into());
    }
    if o.ledger.document() != source.document() {
        return Err("the documents differ at the head".into());
    }
    let history = o.ledger.history().ok_or("no history")?;
    for tx in source.log() {
        let got = history.transaction(tx.seq).map_err(|e| e.to_string())?;
        if &got != tx {
            return Err(format!("transaction {} differs", tx.seq));
        }
    }
    let r = o.project.store().reader().map_err(|e| e.to_string())?;
    let mut named = 0;
    for kind in [VersionKind::User, VersionKind::Revision] {
        for v in r.versions(o.project.branch(), kind).map_err(|e| e.to_string())? {
            let doc = history.state_at(v.point.seq).map_err(|e| e.to_string())?;
            if doc != source.state_at(v.point.seq).map_err(|e| e.to_string())? {
                return Err(format!("the snapshot of version {:?} differs", v.name));
            }
            named += 1;
        }
    }
    if named != source.document().versions.len() {
        return Err(format!("{named} versions were written, the source names {}", source.document().versions.len()));
    }
    Ok(named)
}
