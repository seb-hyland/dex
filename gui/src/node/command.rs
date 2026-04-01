use crate::prelude::*;
use crate::{canvas::NodeIdx, node::Node};

pub enum CanvasCommand {
    AddNode { node: Node },
    MoveNode { idx: NodeIdx, delta: Vec2 },
    AddEdge { start: NodeIdx, end: NodeIdx },
}
