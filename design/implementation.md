# Visualization

## Primitives

- Basic egui widgets, fully implemented on Rust side (blackbox)
- Fully self-contained with a defined size

## Composites

- For at least some:
  - Defined in a scripting/extension language -> Python
  - Able to be modified at runtime
- Has internal layouting, "tells" internal primitives where to draw
- Either canvas (zstack), vertical (ystack), or horizontal (xstack)

## Layouting, multi-parenting, history

- Nodes may have a "global" (within workspace) UID
  - This is required if messaging/mutation/interaction is desired
  - UIDs are generated deterministically by workplace state, requested via action (replication)
  - The workspace contains a lookup map that holds a reference to the node, for messaging
  - The nodes themselves are stored in a flat "pool" for serialization
    - Every node from every time period exists in this pool
  - Each node is reference counted, with two references in the current time period (exists in the lookup map and held by the parent)
  - Nodes are passed their own UID when drawn
- On draw, each parent draws its children (in a tree), passing constraints to children (location, size, etc.)
  - Metadata is stored such that "last known location/size" can be queried (or none can be handled)
    - This enables _relative_ constraints (e.g., 5 pixels right of node x, connecting lines)
- Histories are grained both at node-level and workspace-level
  - A history buffer exists at the workspace level, and at the node level
  - The entire node tree exists in a persistent structurally-shared data structure
    - Any point in the history tree can be accessed
    - Rollback/forward will jump local histories to that point as well
    - Histories have an associated UUID generated when a message is sent
      - This UUID is recorded in both the global and local history to identify them
      - Time-ordered UUID

## Messaging

### Queries

Synchronous operations that may request data from a node

- They are non-mutating and can be called during draw operations
- Every node supports certain queries, and returns `None` in cases where the query is not understood

### Actions

Requested history-defining operations

- Transient mutations that can be discarded/reconstructed are done via queries
- Asynchronous and do not have a return value
- Buffered and processed at frame end

### Multi-language support

- All queries and messages must implement the `Query`/`Message` trait, which supertraits a trait for boundary passing
- This way, they can be defined in Rust or a scripting language

# Control flow

- Control-flow lines can be created between any node and a transform node's argument node
- A global control-flow graph exists: creating a control-flow line updates it
- When `exec` is called on a transform (asynchronous action run in background thread), it executes, then
  informs the control-flow graph where the execution occured and the modified result node
  - The result node may have been replaced
  - The control-flow graph requests execution on downstream nodes

## Background executor

- Execution is a function that returns a `Vec<Action>` (the execution effects)
- The workspace holds a "blackboard" of mutated nodes
  - Helps with invalidation of arbitrary other nodes that care
    - Lambdas can check for invalidation of their output
  - Blackboard is "wiped" every few frames? (todo: is 1 frame okay, cleaned at end?)
- The execution system must be able to map the requester node to each computation
  - Requester nodes must be able to kill their previous computations if a new invalidation has occured
  - Effects will be commited simultaneously at compute-end (transaction) IFF no kill flag has been set

# Saving

- Ref-counted values require custom serialization so as not to duplicate on write
- Strategy:
  - On write:
    - Global lookup map, mapping pointers to serializable values
    - Check if pointer already exists in map
    - If not, serialize and add to map
    - Replace `<this>` with integer value of pointer
  - On read:
    - Make map of serialization-time pointer -> (value, materialized rc)
    - If rc materialized already, clone, else create
