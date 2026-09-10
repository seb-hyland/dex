"""Two views of one table, joined record by record.

Wire a `Table` into `thisData`. This builds a scatter and a violin over the
*same* `Frame` — read once, plotted twice — puts them side by side on a plane,
and draws a line between every record in one and the same record in the other.

The joining is the interesting part, and it is four lines: ask both views where
they drew each row this frame, and connect the pairs. Neither view is told the
other exists. `Correspondence` is written against the prelude's protocol and
nothing else, so the same code joins a tree to a circos, or a 3D cloud to a bar
chart, without a line changing.

Two things make that work. Every layout answers `DrawnPoints` in *screen*
coordinates, mapped out of whatever plane it sits on, so two views that share no
frame of reference still agree about where a point is. And every layout keys its
marks by row id, so "the same record" needs no configuration when both came from
one table — and needs only a key column named in the `Encoding` when they did
not.

Click a point in either view and both light up: `mirror_selection` pushes the
selection across each frame, which is the write half of the same protocol.
"""

#: How wide each view is drawn, and how much air between them.
VIEW = (620.0, 700.0)
GAP = 60.0


class LinkedPair:
    """Two views side by side, with the lines between them drawn over the top.

    It owns nothing but ids and a `Correspondence`: the views place themselves,
    and this only says where. Drawing the joiner last is what matters — it asks
    both views where they drew this frame, so it has to be drawn after they
    have.
    """

    def __init__(self, left, right, joiner):
        self.left = left
        self.right = right
        self.joiner = joiner
        #: What `sync_selection` last settled on, so it can tell which view
        #: changed this frame. See its docstring.
        self.sync = None

    def owned_nodes(self):
        return [self.left, self.right, self.joiner]

    def on_delete(self, ctx):
        for uid in self.owned_nodes():
            ctx.workspace.delete_node(uid)

    def type_name(self):
        return "A Linked Pair"

    def request(self, req, ctx):
        """Questions go to the left view, so a pair answers like a single one."""
        if getattr(req, "name", None) in PROTOCOL:
            return workspace_of(ctx).send_request(self.left, req)
        return NotImplemented

    def draw(self, ctx):
        base = ctx.constraints
        (x, y) = (base.pos.x, base.pos.y)
        box = at(x, y, VIEW[0], VIEW[1])
        ctx.draw_inspectable_node(self.left, box)
        ctx.draw_inspectable_node(
            self.right, at(x + VIEW[0] + GAP, y, VIEW[0], VIEW[1]))

        # Clicking either view selects in both, and clearing either clears
        # both — two-way, so a linked pair can be put away. See `sync_selection`.
        ws = ctx.node.workspace
        self.sync = sync_selection(ws, self.left, self.right, self.sync)

        # Last, and over everything: it reads what the two just recorded.
        width = VIEW[0] * 2.0 + GAP
        ctx.draw_node(self.joiner, at(x, y, width, VIEW[1]))
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(
                base.pos, dex.Vector.new(width, VIEW[1])))


def transform():
    """A scatter and a violin of `thisData`, joined."""
    ws = dex.ws
    frame = Frame(globals().get("thisData"))
    continuous = frame.continuous()
    if len(continuous) < 2:
        raise ValueError("this wants a table with two numeric columns")
    categorical = frame.categorical()

    scatter = ws.insert_node_dyn(build_plot(
        ws, Scatter, frame=frame,
        x=continuous[0], y=continuous[1],
        color=categorical[0] if categorical else None))
    violin = ws.insert_node_dyn(build_plot(
        ws, Violin, frame=frame,
        x=categorical[0] if categorical else continuous[0], y=continuous[1]))

    joiner = Correspondence(scatter, violin)
    # Every line at once would be a grey rectangle; the selected record is what
    # anybody actually wants to follow across.
    joiner.only_selected = True

    pair = ws.insert_node_dyn(
        LinkedPair(scatter, violin, ws.insert_node_dyn(joiner)))
    return on_plane(ws, pair, (VIEW[0] * 2.0 + GAP, VIEW[1]), "Linked views")
