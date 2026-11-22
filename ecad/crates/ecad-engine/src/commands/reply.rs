//! What a command's reply says, read as the types it holds. A reply is JSON for the agent and the
//! command line; the app reads it here and never parses an ID, a number or a word out of it.
//!
//! The ID types serialise as their hex text, so reading one is deserialising it.

use serde::de::DeserializeOwned;
use serde_json::Value;

/// The object ID at `key`, when the reply has one.
///
/// # Semantics
/// `None` when the key is missing, null, or not an ID of type `T`.
///
/// # Example
/// ```
/// let v = serde_json::json!({"page": "00000000000000000000000000000001"});
/// assert!(ecad_engine::commands::reply::id::<ecad_model::PageId>(&v, "page").is_some());
/// ```
#[must_use]
pub fn id<T: DeserializeOwned>(reply: &Value, key: &str) -> Option<T> {
    serde_json::from_value(reply.get(key)?.clone()).ok()
}

/// The object IDs listed at `key` (those that are not IDs of type `T` are left out).
#[must_use]
pub fn ids<T: DeserializeOwned>(reply: &Value, key: &str) -> Vec<T> {
    reply.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| serde_json::from_value(x.clone()).ok()).collect()).unwrap_or_default()
}

/// `newpage`'s reply: the page made and its number as shown.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct NewPage {
    pub page: ecad_model::PageId,
    pub number: String,
}

/// The reply of any command that names the page it ended on (`openpage`, `newpage`).
#[must_use]
pub fn new_page(reply: &Value) -> Option<NewPage> {
    serde_json::from_value(reply.clone()).ok()
}
