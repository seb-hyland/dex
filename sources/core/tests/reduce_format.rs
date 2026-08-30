//! The `__reduce__` payload format must survive a bound type gaining or
//! reordering a field, since those bytes live inside persisted workspaces.

use dex_core::scripting::{reduce_from_bytes, reduce_to_bytes};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Before {
    a: u32,
    name: String,
}

/// The same type after a field is added.
#[derive(Serialize, Deserialize)]
struct AfterAdding {
    a: u32,
    name: String,
    #[serde(default)]
    added: u32,
}

/// The same type after its fields are reordered.
#[derive(Serialize, Deserialize)]
struct AfterReordering {
    name: String,
    a: u32,
}

#[test]
fn captured_bytes_survive_a_struct_changing() {
    let bytes = reduce_to_bytes(&Before {
        a: 7,
        name: "hello".to_owned(),
    })
    .expect("captures");

    let added: AfterAdding = reduce_from_bytes(&bytes).expect("tolerates an added field");
    assert_eq!(added.a, 7);
    assert_eq!(added.name, "hello");
    assert_eq!(added.added, 0, "a new field takes its default");

    let reordered: AfterReordering = reduce_from_bytes(&bytes).expect("tolerates reordered fields");
    assert_eq!(reordered.a, 7);
    assert_eq!(reordered.name, "hello");
}

/// Binary, not text: the payload is opaque and should not be paying for JSON.
#[test]
fn captured_bytes_are_compact() {
    let bytes = reduce_to_bytes(&Before {
        a: 7,
        name: "hello".to_owned(),
    })
    .expect("captures");
    let as_json = serde_json::to_vec(&Before {
        a: 7,
        name: "hello".to_owned(),
    })
    .unwrap();
    assert!(
        bytes.len() < as_json.len(),
        "{} bytes is not smaller than JSON's {}",
        bytes.len(),
        as_json.len()
    );
}
