//! Loading a workspace saved *before* the "Reset all canvases" button existed
//! must still work. The `reset_canvases_button` field carries a `serde(default)`,
//! so a serialization that lacks it deserialises to a nil id instead of failing
//! with `missing field \`reset_canvases_button\``, and the sidebar rebuilds the
//! button on its next tick.
//!
//! The runtime load path is derived serde over ciborium; JSON here exercises the
//! same generated `Deserialize`, and lets the field be stripped as text.

use dex_core::prelude::*;
use dex_nodes::layouts::desktops::Desktops;

fn sidebar_id(ws: &Workspace) -> NodeUid {
    ws.live_ids()
        .into_iter()
        .find(|&id| {
            ws.get_node(id)
                .map(|n| n.type_name(NodeContext { id, workspace: ws }) == "A Canvas Sidebar")
                .unwrap_or(false)
        })
        .expect("a desktop workspace has a sidebar")
}

#[test]
fn a_workspace_saved_before_the_reset_button_still_loads() {
    let ws = Desktops::new_workspace();
    let node = ws
        .get_node(sidebar_id(&ws))
        .expect("the sidebar is live");

    // Serialise the sidebar the way a save does (typetag tags it under its type
    // name), then strip the new field to stand in for an older save.
    let mut json = serde_json::to_value(&*node).expect("the sidebar serialises");
    let body = json
        .as_object_mut()
        .and_then(|obj| obj.values_mut().next()) // typetag wraps fields under the tag
        .and_then(|tagged| tagged.as_object_mut())
        .expect("a tagged object with a field body");
    assert!(
        body.remove("reset_canvases_button").is_some(),
        "the field is present to strip — otherwise this test proves nothing"
    );

    // Before the `serde(default)`, this failed with a missing-field error, which
    // surfaced as \"not a dex workspace\" on load.
    let restored: Box<dyn Node> =
        serde_json::from_value(json).expect("an old save loads without the new field");

    // And it round-trips to the same node type, now with the field defaulted.
    let mut round = Workspace::new_empty();
    let id = round.action_handle().insert_node_dyn(restored.into());
    round.process_pending();
    assert_eq!(
        round
            .get_node(id)
            .expect("the restored sidebar is live")
            .type_name(NodeContext {
                id,
                workspace: &round,
            }),
        "A Canvas Sidebar",
        "the sidebar deserialised back to itself"
    );
}
