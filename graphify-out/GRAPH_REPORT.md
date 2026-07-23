# Graph Report - .  (2026-07-23)

## Corpus Check
- 9 files · ~6,122 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 329 nodes · 618 edges · 25 communities (20 shown, 5 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 16 edges (avg confidence: 0.87)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Label Rendering
- Node & Action Model
- Geometry & Positioning
- Draw Constraints & Workspace
- Reactive Cells
- Design Concepts & Architecture
- Shape Primitives
- History Graph
- Request/Response Protocol
- Interaction Handling
- Canvas Layout
- Horizontal Layout
- Any / Downcasting
- Reset-Derive Macro
- Introspection & Runtime Modification
- Serve Script
- DynClone Trait
- From Conversion
- Response Alias

## God Nodes (most connected - your core abstractions)
1. `Vector` - 26 edges
2. `Node` - 21 edges
3. `ScreenPos` - 20 edges
4. `ScreenRegion` - 18 edges
5. `DrawResult` - 17 edges
6. `DrawContext` - 17 edges
7. `CanvasLayout` - 15 edges
8. `Action` - 14 edges
9. `NodePool` - 13 edges
10. `Workspace` - 12 edges

## Surprising Connections (you probably didn't know these)
- `Node UID` --semantically_similar_to--> `Data Loading`  [INFERRED] [semantically similar]
  design/implementation.md → design/experience.md
- `RequestBody` --inherits--> `AsAny`  [EXTRACTED]
  sources/workspace/src/messages/request.rs → sources/utils/src/any.rs
- `NodeObject` --references--> `HistoryGraph`  [EXTRACTED]
  sources/workspace/src/pool.rs → sources/utils/src/history.rs
- `Registry` --references--> `HistoryGraph`  [EXTRACTED]
  sources/workspace/src/pool.rs → sources/utils/src/history.rs
- `Request` --references--> `NodeUid`  [EXTRACTED]
  sources/workspace/src/messages/request.rs → sources/workspace/src/pool.rs

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **Messaging Protocol (Queries, Actions, Trait)** — design_implementation_messaging, design_implementation_queries, design_implementation_actions, design_implementation_query_message_trait [EXTRACTED 1.00]
- **Control-Flow Execution Flow** — design_experience_control_flow_line, design_implementation_control_flow_graph, design_implementation_exec, design_experience_transforms [INFERRED 0.85]
- **Node Identity and Persistence Model** — design_implementation_node_uid, design_implementation_node_pool, design_implementation_reference_counting, design_implementation_persistent_data_structure [INFERRED 0.85]

## Communities (25 total, 5 thin omitted)

### Community 0 - "Label Rendering"
Cohesion: 0.08
Nodes (21): DynClone, FontId, Hash, Label, ActionBody, Any, Box, Color32 (+13 more)

### Community 1 - "Node & Action Model"
Cohesion: 0.12
Nodes (24): ActionDescription, HashTrieMap, NodeRef, SlotMap, AsAny, Node, Requestable, Action (+16 more)

### Community 2 - "Geometry & Positioning"
Cohesion: 0.14
Nodes (13): Add, Div, From, Output, PositionConstraint, Pos2, Rect, Self (+5 more)

### Community 3 - "Draw Constraints & Workspace"
Cohesion: 0.10
Nodes (17): Action, Rect, Registry, Request, Resp, AxisConstraint, DrawConstraints, Option (+9 more)

### Community 4 - "Reactive Cells"
Cohesion: 0.14
Nodes (14): Default, Rc, Cell, Cell<T>, Reset, Rigid, Rigid<T>, Clone (+6 more)

### Community 5 - "Design Concepts & Architecture"
Cohesion: 0.11
Nodes (27): Canvas Layout, Composites, Control Flow, Control Flow Line, Data Loading, DSL Bindings / Multi-language Support, History, McCarthy's Paper (+19 more)

### Community 6 - "Shape Primitives"
Cohesion: 0.22
Nodes (12): Circle, Line, Rect, ActionBody, Any, Box, Color32, Option (+4 more)

### Community 7 - "History Graph"
Cohesion: 0.18
Nodes (12): Directed, Edge, NodeIndex, Epoch, Epoch<T>, HistoryGraph, HistoryGraph<T, Edge>, Self (+4 more)

### Community 8 - "Request/Response Protocol"
Cohesion: 0.22
Nodes (15): Response, downcast_resp(), Request, Requestable, RequestBody, Any, Box, From (+7 more)

### Community 9 - "Interaction Handling"
Cohesion: 0.16
Nodes (14): Sense, InteractionBox, LastFrameInteractions, ActionBody, Any, Box, Option, Requestable (+6 more)

### Community 10 - "Canvas Layout"
Cohesion: 0.17
Nodes (11): CanvasLayout, CanvasNode, ActionBody, Any, Box, NodeUid, Option, Requestable (+3 more)

### Community 11 - "Horizontal Layout"
Cohesion: 0.18
Nodes (9): HorizontalLayout, ActionBody, Any, Box, NodeUid, Option, Requestable, RequestBody (+1 more)

### Community 12 - "Any / Downcasting"
Cohesion: 0.32
Nodes (4): Any, Box, Self, T

### Community 13 - "Reset-Derive Macro"
Cohesion: 0.50
Nodes (3): reset_derive(), Structure, TokenStream

## Knowledge Gaps
- **5 isolated node(s):** `serve.sh script`, `Epoch<T>`, `McCarthy's Paper`, `Control Flow Line`, `Data Loading`
  These have ≤1 connection - possible missing edges or undocumented components.
- **5 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Node` connect `Node & Action Model` to `Label Rendering`, `Draw Constraints & Workspace`, `Shape Primitives`, `Interaction Handling`, `Canvas Layout`, `Horizontal Layout`?**
  _High betweenness centrality (0.317) - this node is a cross-community bridge._
- **Why does `ScreenRegion` connect `Geometry & Positioning` to `Label Rendering`, `Node & Action Model`?**
  _High betweenness centrality (0.140) - this node is a cross-community bridge._
- **What connects `serve.sh script`, `Epoch<T>`, `McCarthy's Paper` to the rest of the system?**
  _5 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Label Rendering` be split into smaller, more focused modules?**
  _Cohesion score 0.08013937282229965 - nodes in this community are weakly interconnected._
- **Should `Node & Action Model` be split into smaller, more focused modules?**
  _Cohesion score 0.12051282051282051 - nodes in this community are weakly interconnected._
- **Should `Geometry & Positioning` be split into smaller, more focused modules?**
  _Cohesion score 0.14453781512605043 - nodes in this community are weakly interconnected._
- **Should `Draw Constraints & Workspace` be split into smaller, more focused modules?**
  _Cohesion score 0.09523809523809523 - nodes in this community are weakly interconnected._