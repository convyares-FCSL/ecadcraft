//! `plcchannel`: one channel's data. The rules are
//! [`super::plan_channel`]; the command is generic ([`crate::commands::run`]).

use std::collections::BTreeMap;

use ecad_model::ConnectionPointId;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::{ChannelInput, channel_number, plan_channel};
use crate::commands::input::{TextOrNumber, nullable};
use crate::commands::run::{Cx, JsonGeneric, Outcome};
use crate::error::CmdError;

/// Sets one PLC channel's data in one transaction. `move_to` gives the points of one drawn copy of the channel
/// (`connection_point`, or the last-drawn copy) the channel with that number.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlcchannelParams {
    /// The card: an ID or a tag.
    card: String,
    /// The channel number, as shown.
    #[serde(default)]
    channel: Option<TextOrNumber>,
    /// The 0-based channel index (stands for `channel` when that is absent).
    #[serde(default)]
    index: Option<TextOrNumber>,
    /// The channel's description; null clears it.
    #[serde(default, deserialize_with = "nullable")]
    text: Option<Option<String>>,
    /// A typed address; null clears it.
    #[serde(default, deserialize_with = "nullable")]
    address: Option<Option<String>>,
    /// The PLC tag; null clears it.
    #[serde(default, deserialize_with = "nullable")]
    tag: Option<Option<String>>,
    /// The safe state (outputs); null clears it.
    #[serde(default, deserialize_with = "nullable")]
    safe: Option<Option<String>>,
    /// New pins: {role: pin}.
    #[serde(default, deserialize_with = "nullable")]
    pins: Option<Option<BTreeMap<String, String>>>,
    /// Give a drawn copy of the channel this channel number.
    #[serde(default, deserialize_with = "nullable")]
    move_to: Option<Option<TextOrNumber>>,
    /// With `move_to`: a connection point of the copy to move (absent: the last drawn).
    #[serde(default)]
    connection_point: Option<ConnectionPointId>,
}

impl JsonGeneric for PlcchannelParams {
    const ID: &'static str = "plcchannel";
    const TITLE: &'static str = "Channel";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let number = |t: Option<TextOrNumber>| t.map(|t| channel_number(&t.text())).transpose();
        let channel = number(self.channel)?;
        // `index` (0-based, Properties ▸ Channel's pin rows) stands for `channel` when that is absent.
        let index = number(self.index)?;
        if channel.is_none() && index.is_none() {
            return Err(CmdError::refused("`channel` (a number as shown) is required"));
        }
        let values: Vec<(&'static str, Option<String>)> =
            [("text", self.text), ("address", self.address), (crate::plc::signals::TAG, self.tag), (crate::plc::signals::SAFE, self.safe)]
                .into_iter()
                .filter_map(|(what, v)| v.map(|v| (what, v)))
                .collect();
        let input = ChannelInput {
            channel,
            index,
            values,
            pins: self.pins,
            move_to: self.move_to.map(|m| m.map(|t| t.text())),
            connection_point: self.connection_point,
        };
        let plan = plan_channel(cx.doc, &self.card, &input)?;
        let (c, shown) = (plan.card, plan.shown);
        if plan.ops.is_empty() {
            return Ok(Outcome::answer(json!({"card": c.hex(), "channel": shown, "changed": false})).echo_message());
        }
        let message = format!("{} channel {shown}: {}.", plan.who, plan.said.join(", "));
        Ok(Outcome::edit("Channel data", plan.ops, json!({"card": c.hex(), "channel": shown, "changed": true, "message": message})).echo_message())
    }
}
