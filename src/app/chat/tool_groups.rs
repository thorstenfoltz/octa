//! How the assistant's tool payload is kept small.
//!
//! The definitions of all 62 tools are ~99 KB of description and JSON schema,
//! about 33k tokens, and the provider keeps no state between requests: every
//! request in a turn carries the whole set again. A question answered by one
//! `run_sql` call was therefore paying for `fuzzy_join`, `detect_pii` and
//! `partition_table` twice over.
//!
//! So the core group goes out in full, and everything else is named (but not
//! described) by the `enable_tools` meta-tool. When the model needs one it
//! asks for that group, and the group's real definitions join the turn. The
//! menu costs a few hundred bytes; the groups it stands in for cost ~26k
//! tokens per request.
//!
//! The user can switch any individual tool off in Settings > Chat / Assistant,
//! which removes it from the payload, from the menu, and from dispatch.
//!
//! The catalogue itself is [`crate::mcp::tool_groups`], shared with the MCP
//! server's `--mcp-tools`.

use std::collections::BTreeSet;

pub use crate::mcp::tool_groups::{CATALOG, ToolGroup, names_in, write_tool_names};

use super::types::ToolDef;

/// Roughly how many tokens a tool definition costs on the wire.
///
/// Bytes of description plus serialised schema, divided by three. JSON with
/// short identifiers tokenises near that, and the number is labelled as
/// approximate everywhere it is shown: it exists so the settings list can say
/// which tools are expensive, not to reconcile with an invoice.
pub fn approx_tokens(def: &ToolDef) -> usize {
    let schema = serde_json::to_string(&def.input_schema).unwrap_or_default();
    (def.description.len() + schema.len()) / 3
}

/// The tools of one group that survive the user's switches and the profile's
/// write permission, in catalogue order.
pub fn group_members(
    group: ToolGroup,
    disabled: &BTreeSet<String>,
    allow_writes: bool,
) -> Vec<&'static str> {
    names_in(group)
        .into_iter()
        .filter(|name| !disabled.contains(*name))
        .filter(|name| allow_writes || !write_tool_names().contains(name))
        .collect()
}

/// Hand the Settings dialog the tool list it shows.
///
/// Called once at GUI startup. The dialog lives in the library and the tools
/// live here, so this is the seam between them; sizes are measured from the
/// real definitions rather than written down twice.
pub fn publish_to_settings() {
    let defs = super::tools::tool_defs();
    let infos = CATALOG
        .iter()
        .filter_map(|entry| {
            let def = defs.iter().find(|d| d.name == entry.name)?;
            Some(octa::ui::settings::chat_tools::ToolInfo {
                name: entry.name,
                group: entry.group.id(),
                group_summary: entry.group.summary(),
                always_sent: entry.group == ToolGroup::Core,
                description: def.description.clone(),
                when: entry.when,
                tokens: approx_tokens(def),
            })
        })
        .collect();
    octa::ui::settings::chat_tools::publish(infos);
}
