//! `plcaddress` (brief 045, design 045 §5): a rack's address style and starts, a card's pinned
//! start. The rules are [`crate::plc::address`].

use cadcraft_engine::Session;

use crate::cmd::with_project;
use crate::defaults::{self};
use crate::plc::address::*;
use crate::plc::rack::{self, RackRef};
use ecad_model::{ComponentId, ObjectRef, Op};
use serde_json::{Value, json};

/// `plcaddress {rack | card, style?, input_start?, output_start?, start?, size?, fix?, clear?}`
/// (design 045 §5.3). One transaction.
pub(crate) fn run_plcaddress(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let mut ops = Vec::new();
        let set = |ops: &mut Vec<Op>, c: ComponentId, key: &str, after: Option<String>| {
            let before = doc.components.get(&c).and_then(|x| x.properties.get(key)).cloned();
            if before != after {
                ops.push(Op::SetProperty { object: ObjectRef::Component(c), key: key.into(), before, after });
            }
        };
        let word = |v: &Value| -> Result<Option<String>, String> {
            match v {
                Value::Null => Ok(None),
                Value::String(s) => Ok(Some(s.trim().to_string())),
                Value::Number(n) => Ok(Some(n.to_string())),
                _ => Err("a value must be text, a number, or null".into()),
            }
        };
        let (who, rack_id, message_tail) = if let Some(rv) = params.get("rack").filter(|v| !v.is_null()) {
            let r = rack::rack_param(doc, rv)?;
            let before = snapshot(doc, r);
            for (key, entry) in [
                ("style", &defaults::plc::ADDRESS_STYLE),
                ("input_start", &defaults::plc::START_INPUT),
                ("output_start", &defaults::plc::START_OUTPUT),
            ] {
                let Some(v) = params.get(key) else { continue };
                let value = word(v)?;
                if let Some(x) = &value
                    && entry.parse(x).is_none()
                {
                    return Err(format!("`{key}` {x} is not a value of {}.", entry.key));
                }
                set(&mut ops, r, entry.key, value);
            }
            let fix = params.get("fix").and_then(Value::as_bool).unwrap_or(false);
            let clear = params.get("clear").and_then(Value::as_bool).unwrap_or(false);
            if fix && clear {
                return Err("`fix` pins every card; `clear` unpins them: give one.".into());
            }
            let mut pinned_n = 0;
            if fix || clear {
                for (c, b) in &before {
                    let start = b.input.or(b.output).map(|x| x.0);
                    let after = if fix { start.map(|x| x.to_string()) } else { None };
                    let had = ops.len();
                    set(&mut ops, *c, START_KEY, after);
                    pinned_n += usize::from(ops.len() > had);
                }
            }
            let tail = if fix {
                format!("{pinned_n} card{} pinned at their current addresses", if pinned_n == 1 { "" } else { "s" })
            } else if clear {
                format!("{pinned_n} card{} unpinned", if pinned_n == 1 { "" } else { "s" })
            } else {
                String::new()
            };
            (crate::devices::tag_words(doc, r), Some(r), tail)
        } else {
            let c = rack::card_param(doc, params.get("card"))?;
            for (key, prop, max) in [("start", START_KEY, MAX_BYTE), ("size", SIZE_KEY, MAX_SIZE)] {
                let Some(v) = params.get(key) else { continue };
                let value = word(v)?;
                if let Some(x) = &value
                    && !(x.bytes().all(|b| b.is_ascii_digit())
                        && x.len() <= 5
                        && x.parse::<u32>().is_ok_and(|n| n <= max && (key == "start" || n >= 1)))
                {
                    return Err(format!("`{key}` is a whole number of bytes from {} to {max}.", u32::from(key == "size")));
                }
                set(&mut ops, c, prop, value);
            }
            let r = match rack::rack_ref(doc, c) {
                RackRef::Rack(r) => Some(r),
                _ => None,
            };
            (crate::devices::tag_words(doc, c), r, String::new())
        };
        if ops.is_empty() {
            return Ok(json!({"changed": false, "message": format!("{who}: nothing to change.")}));
        }
        let before = rack_id.map(|r| snapshot(doc, r)).unwrap_or_default();
        p.apply_edit(pg, "plcaddress", "Addresses", ops)?;
        let doc = p.ledger.document();
        let after = rack_id.map(|r| snapshot(doc, r)).unwrap_or_default();
        let moved = after.iter().filter(|(c, b)| before.get(c).is_some_and(|x| x.input != b.input || x.output != b.output)).count();
        let mut parts = Vec::new();
        if !message_tail.is_empty() {
            parts.push(message_tail);
        }
        parts.push(format!("addresses of {moved} card{} change", if moved == 1 { "" } else { "s" }));
        let message = format!("{who}: {}.", parts.join("; "));
        Ok(json!({"changed": true, "moved": moved, "message": message}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}
