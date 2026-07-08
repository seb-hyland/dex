# Principles

- Interaction should be intuitive and easy for newcomers without requiring extensive prerequisite knowledge
  - The system should help users "spiral up" into more advanced usage
- Users should be able to introspect/reflect on standard features as much as possible
  - This helps them develop their own capabilities
- Where possible, modification at runtime is preferred
  - "Develop the system in the system"
- State transitions should be done via **messaging** rather than direct modification
  - Derived from [McCarthy's paper](https://apps.dtic.mil/sti/tr/pdf/AD0785031.pdf)
  - History can be represented as a tree of messages, and thereby reconstructed from an initial empty state
  - Some modifications will require local change which are later "committed" via a message (e.g., drag-and-drop will need to have some local state saved, but "history" will only update on drop)

# Features

## Visualization

The ability to construct arbitrary visual representations (both programmatically, and, importantly, _visually_)

- This will require users to be able to layout basic primitives, as well as composites
  - Primitives:
    - Text (label)
    - Text (editable)
    - Text (code editor)
    - Integers
    - Floats
    - Images
    - Basic shapes
      - Rectangle
      - Line
      - **Control flow line**
      - Curve (Bézier)
  - Composites:
    - **Transforms**
    - Tables (dataframes)
    - **Canvas layout**
      - **Subcanvas**
      - Canvas workspace
    - Vertical layout
    - Horizontal layout
    - Flex layouts
- Layout will need to be able to be updated visually (clicking/dragging, buttons to add new nodes) as well as programmatically
- DSL bindings to all supported scripting languages is required
  - Ideally, users can work across any supported language to define custom composite nodes
  - Composite definition requires:
    - The ability to compose primitive visual elements
    - The ability to trigger message sends to other nodes
      - Interaction handling (click/drag/etc.)
      - Requires the ability to query nodes by ID

## Control flow

Certain 'special' visual elements for control flow (edges + transform nodes)

- Transforms should have labeled arguments and ways to constrain based on type + arbitrary constraints?
  - For instance column types, or running a function to determine if an argument is valid
- Control should be able to be started from any point, with functionality to "flow" until it is resolved
  - Asynchronous execution MUST be supported (both background-actor and distributed)
  - Updating state should be done from background thread via messaging

## Data loading

The ability to load data and create many views that reference the underlying data

- Maybe through multiple parenting on 'invisible data-holding' nodes?

## History

The ability to roll forward and backward through history

- Tree view to interop history
  - "Bookmarks" to identify key points in history

## Saving

The ability to save top-level "window elements" (canvas, canvas workspace) to file

- Requires serialization of all persistant structures
- Need to bundle data alongside OR as reference
