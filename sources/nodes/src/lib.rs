// Lets macro-generated `::dex_nodes::...` paths resolve within this crate.
extern crate self as dex_nodes;

pub mod composites;
pub mod layouts;
pub mod primitives;
pub mod scripting;
