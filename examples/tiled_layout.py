HORIZONTAL = "horizontal"
VERTICAL = "vertical"

# Stored as plain RGB triples rather than `dex.Color` values: the palette is
# just data, and building the colour at draw time keeps this node picklable.
PALETTE = [
    (232, 110, 110),  # red
    (232, 168, 96),   # orange
    (226, 208, 104),  # yellow
    (128, 200, 128),  # green
    (104, 176, 224),  # blue
    (160, 136, 216),  # violet
]

# Inset so adjacent tiles read as separate blocks rather than one wash.
TILE_GAP = 2.0


def tile_fractions(n):
    """The fraction of the total area each of `n` children receives, in order."""
    if n <= 0:
        return []
    if n == 1:
        return [1.0]
    first = (n + 1) // 2
    return [f / 2 for f in tile_fractions(first)] + [
        f / 2 for f in tile_fractions(n - first)
    ]


def tile_boxes(n, x, y, width, height, horizontal):
    """`(x, y, w, h)` for each of `n` tiles filling the given box."""
    if n <= 0:
        return []
    if n == 1:
        return [(x, y, width, height)]

    first = (n + 1) // 2
    if horizontal:
        half = width / 2
        near = tile_boxes(first, x, y, half, height, False)
        far = tile_boxes(n - first, x + half, y, width - half, height, False)
    else:
        half = height / 2
        near = tile_boxes(first, x, y, width, half, True)
        far = tile_boxes(n - first, x, y + half, width, height - half, True)
    return near + far


class TiledLayout:
    def __init__(self, children=None, axis=HORIZONTAL, palette=None):
        self.children = list(children or [])
        self.axis = axis
        # `None` means "use the default palette"; `[]` means "no backgrounds".
        self.palette = list(PALETTE if palette is None else palette)

    # -- drawing ---------------------------------------------------------

    def boxes(self, width, height):
        """`(x, y, w, h)` per child, in node-local coordinates."""
        return tile_boxes(
            len(self.children), 0.0, 0.0, width, height, self.axis == HORIZONTAL
        )

    def draw(self, ctx):
        """Paint the tiles and report the area taken, exactly as a Rust node does."""
        base = ctx.constraints
        # An unbounded axis has no extent to divide.
        width = base.x.provided_value() if base.x is not None else None
        height = base.y.provided_value() if base.y is not None else None
        if width is None or height is None or not self.children:
            return dex.DrawResult.Complete(region=None)

        def box_at(x, y, w, h):
            """Constraints for a sub-box at node-local `(x, y)`, sized `w` by `h`."""
            return dex.DrawConstraints(
                pos=dex.ScreenPos.new(base.pos.x + x, base.pos.y + y),
                x=dex.AxisConstraint.Exactly(w),
                y=dex.AxisConstraint.Exactly(h),
                wrap=None,  # NotAllowed
                should_clip=True,
            )

        for i, (child, box) in enumerate(zip(self.children, self.boxes(width, height))):
            x, y, w, h = box

            if self.palette:
                # Inset the background, but never past zero for a thin tile.
                inset = min(TILE_GAP, w / 2, h / 2)
                bg_w = max(w - 2 * inset, 0.0)
                bg_h = max(h - 2 * inset, 0.0)
                r, g, b = self.palette[i % len(self.palette)]
                # `Rect` paints at its own size, so build it to fill the tile.
                ctx.draw_node(
                    dex.Rect.new(bg_w, bg_h, dex.Color.rgb(r, g, b)),
                    box_at(x + inset, y + inset, bg_w, bg_h),
                )

            ctx.draw_node(child, box_at(x, y, w, h))

        # The tiles fill the box we were given, so that is what we occupied.
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(
                base.pos, dex.Vector.new(width, height)
            )
        )

    # -- messages --------------------------------------------------------

    def handle_action(self, action, ctx):
        if isinstance(action, dex.AddChild):
            self.children.append(action.child)
            return True
        if isinstance(action, dex.RemoveChild):
            # `child` is a NodeUid; handles compare by identity of the node.
            self.children = [c for c in self.children if c != action.child]
            return True
        return NotImplemented

    def request(self, req, ctx):
        if isinstance(req, dex.ChildCount):
            return len(self.children)
        return NotImplemented

    # -- lifecycle -------------------------------------------------------

    def type_name(self):
        return "Tiled Layout"

    def owned_nodes(self):
        """The nodes this layout owns, for deleting and for deep cloning.

        Deep cloning uses this to decide what to copy. It does not need to be
        told where the handles live — a handle buried anywhere in this object
        rewrites itself to point at the copy.
        """
        return [child for child in self.children if isinstance(child, dex.NodeUid)]

    def build_inspector(self, ctx):
        """The commands this layout offers the inspector's lens.

        Returns a node — a handle to one built here, or any value, which the
        runtime registers. Whatever comes back belongs to the inspector and is
        deleted when the menu closes, so do not hold on to it.

        Placement is *not* one of them. Copy, Mirror and the rest are added by
        the inspector itself — for an item on a canvas, and for a node that is
        some other node's result — so offering them here too puts every one of
        those rows in the menu twice. What belongs here is whatever this node
        alone can do.
        """
        return None

    def on_delete(self, ctx):
        """Delete the children we own."""
        for child in self.owned_nodes():
            ctx.workspace.delete_node(child)


def transform():
    """A lambda returning a tiled layout of five labelled, coloured tiles."""
    return TiledLayout(
        children=[dex.Label.new(name) for name in ("one", "two", "three", "four", "five")],
        axis=HORIZONTAL,
    )
