//! Dumps the bound API as a `.pyi`, for inspecting or diffing it by hand.
//!
//! Checkouts render their own stubs at runtime, so nothing depends on this
//! being run — it exists to look at.
//!
//! Lives in `dex-nodes` rather than `dex-core` so that linking pulls in every
//! crate's inventory submissions — a generator in `dex-core` would describe
//! only half the API.

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    // Force the node bindings to link; without naming the crate their
    // `inventory::submit!`s are dropped.
    dex_nodes::scripting::init_python();

    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("dex.pyi"));

    let rendered = dex_core::stubs_gen::render();
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&out, &rendered)?;
    println!("wrote {} ({} bytes)", out.display(), rendered.len());
    Ok(())
}
