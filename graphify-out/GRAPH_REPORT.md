# Graph Report - .  (2026-07-23)

## Corpus Check
- 3 files · ~6,145 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 340 nodes · 618 edges · 32 communities (20 shown, 12 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 16 edges (avg confidence: 0.87)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Draw Constraints & Geometry
- Label Rendering
- Node Pool & Registry
- Reactive Cells
- Design Concepts & Architecture
- Any & Action Model
- Shape Primitives
- Workspace & Rendering
- History Graph
- Request/Response Protocol
- Interaction Handling
- Canvas Layout
- Horizontal Layout
- Reset-Derive Macro
- Introspection & Runtime Modification
- Serve Script
- Action Node
- Registry
- DynClone Trait
- Node UID
- Requestable Trait
- From Conversion
- Any Trait
- Node UID (alt)
- Response Alias
- Vec

## God Nodes (most connected - your core abstractions)
1. `Vector` - 26 edges
2. `Node` - 21 edges
3. `ScreenPos` - 20 edges
4. `DrawResult` - 17 edges
5. `DrawContext` - 17 edges
6. `CanvasLayout` - 15 edges
7. `NodeUid` - 13 edges
8. `NodePool` - 13 edges
9. `ScreenRegion` - 12 edges
10. `Workspace` - 12 edges

## Surprising Connections (you probably didn't know these)
- `Node UID` --semantically_similar_to--> `Data Loading`  [INFERRED] [semantically similar]
  design/implementation.md → design/experience.md
- `RequestBody` --inherits--> `AsAny`  [EXTRACTED]
  sources/workspace/src/messages/request.rs → sources/utils/src/any.rs
- `Action` --references--> `NodeUid`  [EXTRACTED]
  sources/workspace/src/messages/action.rs → sources/workspace/src/pool.rs
- `Request` --references--> `NodeUid`  [EXTRACTED]
  sources/workspace/src/messages/request.rs → sources/workspace/src/pool.rs
- `TypedRequest` --references--> `NodeUid`  [EXTRACTED]
  sources/workspace/src/messages/request.rs → sources/workspace/src/pool.rs

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **Messaging Protocol (Queries, Actions, Trait)** — design_implementation_messaging, design_implementation_queries, design_implementation_actions, design_implementation_query_message_trait [EXTRACTED 1.00]
- **Control-Flow Execution Flow** — design_experience_control_flow_line, design_implementation_control_flow_graph, design_implementation_exec, design_experience_transforms [INFERRED 0.85]
- **Node Identity and Persistence Model** — design_implementation_node_uid, design_implementation_node_pool, design_implementation_reference_counting, design_implementation_persistent_data_structure [INFERRED 0.85]

## Communities (32 total, 12 thin omitted)

### Community 0 - "Draw Constraints & Geometry"
Cohesion: 0.11
Nodes (16): Add, Div, From, Output, AxisConstraint, DrawConstraints, PositionConstraint, Option (+8 more)

### Community 1 - "Label Rendering"
Cohesion: 0.08
Nodes (22): DynClone, FontId, Hash, Label, ActionBody, Any, Box, Color32 (+14 more)

### Community 2 - "Node Pool & Registry"
Cohesion: 0.18
Nodes (17): HashTrieMap, HistoryGraph, NodeRef, Requestable, SlotMap, Node, NodeObject, NodePool (+9 more)

### Community 3 - "Reactive Cells"
Cohesion: 0.14
Nodes (14): Default, Rc, Cell, Cell<T>, Reset, Rigid, Rigid<T>, Clone (+6 more)

### Community 4 - "Design Concepts & Architecture"
Cohesion: 0.11
Nodes (27): Canvas Layout, Composites, Control Flow, Control Flow Line, Data Loading, DSL Bindings / Multi-language Support, History, McCarthy's Paper (+19 more)

### Community 5 - "Any & Action Model"
Cohesion: 0.13
Nodes (15): ActionDescription, AsAny, Any, Box, Self, T, Action, ActionBody (+7 more)

### Community 6 - "Shape Primitives"
Cohesion: 0.22
Nodes (12): Circle, Line, Rect, ActionBody, Any, Box, Color32, Option (+4 more)

### Community 7 - "Workspace & Rendering"
Cohesion: 0.13
Nodes (13): Any, Rect, Request, Resp, DrawContext<'ctx>, Action, Box, DrawConstraints (+5 more)

### Community 8 - "History Graph"
Cohesion: 0.18
Nodes (12): Directed, Edge, NodeIndex, Epoch, Epoch<T>, HistoryGraph, HistoryGraph<T, Edge>, Self (+4 more)

### Community 9 - "Request/Response Protocol"
Cohesion: 0.22
Nodes (15): Response, downcast_resp(), Request, Requestable, RequestBody, Any, Box, From (+7 more)

### Community 10 - "Interaction Handling"
Cohesion: 0.16
Nodes (14): Sense, InteractionBox, LastFrameInteractions, ActionBody, Any, Box, Option, Requestable (+6 more)

### Community 11 - "Canvas Layout"
Cohesion: 0.18
Nodes (11): CanvasLayout, CanvasNode, ActionBody, Any, Box, NodeUid, Option, Requestable (+3 more)

### Community 12 - "Horizontal Layout"
Cohesion: 0.18
Nodes (9): HorizontalLayout, ActionBody, Any, Box, NodeUid, Option, Requestable, RequestBody (+1 more)

### Community 13 - "Reset-Derive Macro"
Cohesion: 0.50
Nodes (3): reset_derive(), Structure, TokenStream

## Knowledge Gaps
- **5 isolated node(s):** `serve.sh script`, `Epoch<T>`, `McCarthy's Paper`, `Control Flow Line`, `Data Loading`
  These have ≤1 connection - possible missing edges or undocumented components.
- **12 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Node` connect `Node Pool & Registry` to `Label Rendering`, `Shape Primitives`, `Workspace & Rendering`, `Interaction Handling`, `Canvas Layout`, `Horizontal Layout`?**
  _High betweenness centrality (0.202) - this node is a cross-community bridge._
- **Why does `NodeUid` connect `Node Pool & Registry` to `Label Rendering`, `Any & Action Model`, `Request/Response Protocol`, `Workspace & Rendering`?**
  _High betweenness centrality (0.153) - this node is a cross-community bridge._
- **Why does `Vector` connect `Draw Constraints & Geometry` to `Interaction Handling`, `Canvas Layout`, `Shape Primitives`?**
  _High betweenness centrality (0.110) - this node is a cross-community bridge._
- **What connects `serve.sh script`, `Epoch<T>`, `McCarthy's Paper` to the rest of the system?**
  _5 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Draw Constraints & Geometry` be split into smaller, more focused modules?**
  _Cohesion score 0.11265969802555169 - nodes in this community are weakly interconnected._
- **Should `Label Rendering` be split into smaller, more focused modules?**
  _Cohesion score 0.08048780487804878 - nodes in this community are weakly interconnected._
- **Should `Reactive Cells` be split into smaller, more focused modules?**
  _Cohesion score 0.14285714285714285 - nodes in this community are weakly interconnected._