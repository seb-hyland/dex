# Graph Report - .  (2026-07-23)

## Corpus Check
- Corpus is ~5,707 words - fits in a single context window. You may not need a graph.

## Summary
- 277 nodes · 559 edges · 20 communities (18 shown, 2 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 16 edges (avg confidence: 0.87)
- Token cost: 42,038 input · 0 output

## Community Hubs (Navigation)
- Geometry & Positioning
- Label Rendering
- Interaction Handling
- Reactive Cells
- Design Concepts & Architecture
- Node Pool & Registry
- Actions & Dynamic Dispatch
- Shape Primitives
- History Graph
- Workspace & Rendering
- Horizontal Layout
- Reset-Derive Macro
- Introspection & Runtime Modification
- Serve Script

## God Nodes (most connected - your core abstractions)
1. `Vector` - 24 edges
2. `Node` - 19 edges
3. `ScreenPos` - 19 edges
4. `ScreenRegion` - 18 edges
5. `DrawContext` - 15 edges
6. `Action` - 15 edges
7. `DrawResult` - 14 edges
8. `NodeUid` - 13 edges
9. `NodePool` - 13 edges
10. `ActionBody` - 12 edges

## Surprising Connections (you probably didn't know these)
- `Node UID` --semantically_similar_to--> `Data Loading`  [INFERRED] [semantically similar]
  design/implementation.md → design/experience.md
- `HorizontalLayout` --implements--> `Node`  [EXTRACTED]
  sources/nodes/src/layouts/horizontal.rs → sources/workspace/src/lib.rs
- `HorizontalLayout` --implements--> `Requestable`  [EXTRACTED]
  sources/nodes/src/layouts/horizontal.rs → sources/workspace/src/messages/request.rs
- `HorizontalLayout` --references--> `NodeUid`  [EXTRACTED]
  sources/nodes/src/layouts/horizontal.rs → sources/workspace/src/pool.rs
- `InteractionBox` --references--> `Transient`  [EXTRACTED]
  sources/nodes/src/primitives/interaction.rs → sources/utils/src/cell.rs

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **Messaging Protocol (Queries, Actions, Trait)** — design_implementation_messaging, design_implementation_queries, design_implementation_actions, design_implementation_query_message_trait [EXTRACTED 1.00]
- **Control-Flow Execution Flow** — design_experience_control_flow_line, design_implementation_control_flow_graph, design_implementation_exec, design_experience_transforms [INFERRED 0.85]
- **Node Identity and Persistence Model** — design_implementation_node_uid, design_implementation_node_pool, design_implementation_reference_counting, design_implementation_persistent_data_structure [INFERRED 0.85]

## Communities (20 total, 2 thin omitted)

### Community 0 - "Geometry & Positioning"
Cohesion: 0.14
Nodes (13): Add, Div, Output, PositionConstraint, Pos2, Rect, From, Self (+5 more)

### Community 1 - "Label Rendering"
Cohesion: 0.10
Nodes (15): FontId, Label, Any, Box, Color32, Option, AxisConstraint, DrawConstraints (+7 more)

### Community 2 - "Interaction Handling"
Cohesion: 0.12
Nodes (21): Response, Sense, InteractionBox, LastFrameInteractions, Any, Box, Option, WasClicked (+13 more)

### Community 3 - "Reactive Cells"
Cohesion: 0.15
Nodes (14): Default, Rc, Cell, Cell<T>, Reset, Rigid, Rigid<T>, Clone (+6 more)

### Community 4 - "Design Concepts & Architecture"
Cohesion: 0.11
Nodes (27): Canvas Layout, Composites, Control Flow, Control Flow Line, Data Loading, DSL Bindings / Multi-language Support, History, McCarthy's Paper (+19 more)

### Community 5 - "Node Pool & Registry"
Cohesion: 0.24
Nodes (12): HashTrieMap, NodeRef, SlotMap, Node, NodeObject, NodePool, NodeUid, Registry (+4 more)

### Community 6 - "Actions & Dynamic Dispatch"
Cohesion: 0.12
Nodes (15): ActionDescription, AsAny, Any, Box, Self, T, Action, ActionBody (+7 more)

### Community 7 - "Shape Primitives"
Cohesion: 0.20
Nodes (12): Circle, Line, Rect, Any, Box, Color32, Option, Triangle (+4 more)

### Community 8 - "History Graph"
Cohesion: 0.16
Nodes (12): Directed, Edge, NodeIndex, Epoch, Epoch<T>, HistoryGraph, HistoryGraph<T, Edge>, Self (+4 more)

### Community 9 - "Workspace & Rendering"
Cohesion: 0.17
Nodes (9): Rect, DrawContext<'ctx>, Any, Box, Option, Resp, Ui, Vec (+1 more)

### Community 10 - "Horizontal Layout"
Cohesion: 0.29
Nodes (5): HorizontalLayout, Any, Box, Option, Vec

### Community 11 - "Reset-Derive Macro"
Cohesion: 0.50
Nodes (3): reset_derive(), Structure, TokenStream

## Knowledge Gaps
- **6 isolated node(s):** `serve.sh script`, `Epoch<T>`, `DrawContext<'ctx>`, `McCarthy's Paper`, `Control Flow Line` (+1 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **2 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `InteractionBox` connect `Interaction Handling` to `Reactive Cells`, `Node Pool & Registry`, `Shape Primitives`?**
  _High betweenness centrality (0.143) - this node is a cross-community bridge._
- **Why does `Node` connect `Node Pool & Registry` to `Label Rendering`, `Horizontal Layout`, `Interaction Handling`, `Shape Primitives`?**
  _High betweenness centrality (0.133) - this node is a cross-community bridge._
- **Why does `Transient` connect `Reactive Cells` to `Interaction Handling`?**
  _High betweenness centrality (0.131) - this node is a cross-community bridge._
- **What connects `serve.sh script`, `Epoch<T>`, `DrawContext<'ctx>` to the rest of the system?**
  _6 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Geometry & Positioning` be split into smaller, more focused modules?**
  _Cohesion score 0.14285714285714285 - nodes in this community are weakly interconnected._
- **Should `Label Rendering` be split into smaller, more focused modules?**
  _Cohesion score 0.09803921568627451 - nodes in this community are weakly interconnected._
- **Should `Interaction Handling` be split into smaller, more focused modules?**
  _Cohesion score 0.11576354679802955 - nodes in this community are weakly interconnected._