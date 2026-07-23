# Graph Report - .  (2026-07-23)

## Corpus Check
- 3 files · ~6,414 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 389 nodes · 674 edges · 40 communities (19 shown, 21 thin omitted)
- Extraction: 98% EXTRACTED · 2% INFERRED · 0% AMBIGUOUS · INFERRED: 16 edges (avg confidence: 0.87)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Node & Action Model
- Draw Constraints & Geometry
- Shape Primitives
- Label Rendering
- Workspace & Rendering
- Canvas Layout
- Reactive Cells
- Design Concepts & Architecture
- Any & Request/Response
- Interaction Handling
- History Graph
- Horizontal Layout
- Reset-Derive Macro
- Introspection & Runtime Modification
- Serve Script
- Any
- DynClone
- Requestable
- ActionBody
- NodeUid
- ActionBody
- ActionBody
- NodeUid
- Clone
- Uuid
- Action
- From
- Action
- DrawConstraints
- Resp
- Ui
- String
- Timestamp
- Vec

## God Nodes (most connected - your core abstractions)
1. `Vector` - 20 edges
2. `ScreenPos` - 19 edges
3. `Node` - 19 edges
4. `CanvasLayout` - 19 edges
5. `Workspace` - 16 edges
6. `Action` - 15 edges
7. `NodeUid` - 15 edges
8. `DrawContext` - 14 edges
9. `Registry` - 13 edges
10. `NodePool` - 13 edges

## Surprising Connections (you probably didn't know these)
- `Node UID` --semantically_similar_to--> `Data Loading`  [INFERRED] [semantically similar]
  design/implementation.md → design/experience.md
- `Request` --references--> `NodeUid`  [EXTRACTED]
  sources/workspace/src/messages/request.rs → sources/workspace/src/pool.rs
- `TypedRequest` --references--> `NodeUid`  [EXTRACTED]
  sources/workspace/src/messages/request.rs → sources/workspace/src/pool.rs
- `Messaging` --implements--> `Messaging-based State Transitions`  [INFERRED]
  design/implementation.md → design/experience.md
- `Primitives (egui widgets)` --implements--> `Primitives`  [INFERRED]
  design/implementation.md → design/experience.md

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **Messaging Protocol (Queries, Actions, Trait)** — design_implementation_messaging, design_implementation_queries, design_implementation_actions, design_implementation_query_message_trait [EXTRACTED 1.00]
- **Control-Flow Execution Flow** — design_experience_control_flow_line, design_implementation_control_flow_graph, design_implementation_exec, design_experience_transforms [INFERRED 0.85]
- **Node Identity and Persistence Model** — design_implementation_node_uid, design_implementation_node_pool, design_implementation_reference_counting, design_implementation_persistent_data_structure [INFERRED 0.85]

## Communities (40 total, 21 thin omitted)

### Community 0 - "Node & Action Model"
Cohesion: 0.12
Nodes (24): ActionDescription, Clone, HashTrieMap, HistoryGraph, NodeRef, SlotMap, Node, Requestable (+16 more)

### Community 1 - "Draw Constraints & Geometry"
Cohesion: 0.11
Nodes (16): Add, Div, From, Output, AxisConstraint, DrawConstraints, PositionConstraint, Option (+8 more)

### Community 2 - "Shape Primitives"
Cohesion: 0.11
Nodes (18): AsAny, Circle, Line, Rect, Any, Box, Color32, Option (+10 more)

### Community 3 - "Label Rendering"
Cohesion: 0.09
Nodes (21): FontId, Hash, Label, Any, Box, Color32, Option, Requestable (+13 more)

### Community 4 - "Workspace & Rendering"
Cohesion: 0.10
Nodes (21): Action, DrawConstraints, LocalId, Rect, Registry, Request, Resp, DrawContext<'ctx> (+13 more)

### Community 5 - "Canvas Layout"
Cohesion: 0.11
Nodes (19): Pos2, ScreenPos, CanvasLayout, CanvasNode, ActionBody, Any, Box, DrawContext (+11 more)

### Community 6 - "Reactive Cells"
Cohesion: 0.15
Nodes (14): Default, Rc, Cell, Cell<T>, Reset, Rigid, Rigid<T>, Clone (+6 more)

### Community 7 - "Design Concepts & Architecture"
Cohesion: 0.11
Nodes (27): Canvas Layout, Composites, Control Flow, Control Flow Line, Data Loading, DSL Bindings / Multi-language Support, History, McCarthy's Paper (+19 more)

### Community 8 - "Any & Request/Response"
Cohesion: 0.13
Nodes (20): Response, AsAny, Any, Box, Self, T, downcast_resp(), Request (+12 more)

### Community 9 - "Interaction Handling"
Cohesion: 0.11
Nodes (19): Sense, InteractionBox, LastFrameInteractions, ActionBody, Any, Box, DrawContext, DrawResult (+11 more)

### Community 10 - "History Graph"
Cohesion: 0.16
Nodes (12): Directed, Edge, NodeIndex, Epoch, Epoch<T>, HistoryGraph, HistoryGraph<T, Edge>, Self (+4 more)

### Community 11 - "Horizontal Layout"
Cohesion: 0.17
Nodes (8): HorizontalLayout, Any, Box, Option, Requestable, RequestBody, String, Vec

### Community 12 - "Reset-Derive Macro"
Cohesion: 0.50
Nodes (3): reset_derive(), Structure, TokenStream

## Knowledge Gaps
- **5 isolated node(s):** `serve.sh script`, `Epoch<T>`, `McCarthy's Paper`, `Control Flow Line`, `Data Loading`
  These have ≤1 connection - possible missing edges or undocumented components.
- **21 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `CanvasLayout` connect `Canvas Layout` to `Interaction Handling`, `Reactive Cells`?**
  _High betweenness centrality (0.232) - this node is a cross-community bridge._
- **Why does `Workspace` connect `Workspace & Rendering` to `Label Rendering`?**
  _High betweenness centrality (0.213) - this node is a cross-community bridge._
- **Why does `DrawContext` connect `Label Rendering` to `Shape Primitives`, `Horizontal Layout`, `Workspace & Rendering`?**
  _High betweenness centrality (0.192) - this node is a cross-community bridge._
- **What connects `serve.sh script`, `Epoch<T>`, `McCarthy's Paper` to the rest of the system?**
  _5 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Node & Action Model` be split into smaller, more focused modules?**
  _Cohesion score 0.11846689895470383 - nodes in this community are weakly interconnected._
- **Should `Draw Constraints & Geometry` be split into smaller, more focused modules?**
  _Cohesion score 0.11265969802555169 - nodes in this community are weakly interconnected._
- **Should `Shape Primitives` be split into smaller, more focused modules?**
  _Cohesion score 0.10793650793650794 - nodes in this community are weakly interconnected._