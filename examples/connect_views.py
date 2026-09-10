"""Two visualizations already built, joined record by record.

Wire a view into each of `thisView` and `thatView`. This lays them side by side
and draws a line between every record the two have in common — the same join
`scatterplot.py` uses, but over two views handed in rather than two it builds
itself.

Anything that answers the workspace protocol will do, and — this is the part
that makes it useful — so does the *plane* a data explorer or a phylogeny hands
back. A plane is a `Canvas`, which cannot say where it drew a row; the view on
it can, so the library's join reaches through a plane to the single view it
holds (`view_of`). That is why a data explorer on one column joins to a
phylogeny on another from the same table with nothing declared: the rows carry
the correspondence, and reaching through is all it takes to read it.

**Each wired view is deep-cloned into this node.** A node is drawn in exactly
one place — draw the very same view both where it already sits and here and its
widgets take the same ids twice, which egui paints back as a "use of widget ID"
error. So this copies each view, ids and all, and shows the copies; the
originals stay where they were, untouched, and the pair owns what it made and
takes it with it when deleted. Set `only_selected` to join just the record you
clicked, which is what you want the moment there are more rows than there are
pixels between them.

For lines over two views left exactly where they already sit — no copy, nothing
redrawn — hang a bare `link_views` in a plane's foreground instead. That reads
the originals in place; it works only because a foreground draws each view once.
"""

import math

#: How much air the pair leaves between the two views.
GAP = 60.0


class Connected:
    """The two views side by side, with the join drawn over the top.

    It owns the joiner it made and nothing else — the two views were handed in
    and belong to whoever made them. The links are drawn last, after both views
    have painted, because "where did you draw row 7" is a question about a frame
    that has already happened.
    """

    def __init__(self, left, right, joiner):
        self.left = left
        self.right = right
        self.joiner = joiner
        #: Clicking either view selects in both, so a record found in one is
        #: found in the other.
        self.mirrors = True

    def owned_nodes(self):
        # The two views are this node's own copies, so they go with it: a clone
        # of the pair copies them again, and a delete takes them along.
        return [self.left, self.right, self.joiner]

    def on_delete(self, ctx):
        for uid in self.owned_nodes():
            ctx.workspace.delete_node(uid)

    def type_name(self):
        return "Connected Views"

    def request(self, req, ctx):
        """Answer the protocol as the left view does, so a connected pair is
        itself a view — and can be one half of another pair."""
        if getattr(req, "name", None) in PROTOCOL:
            ws = workspace_of(ctx)
            return ws.send_request(view_of(ws, self.left), req)
        return NotImplemented

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        h = base.y.provided_value() if base.y is not None else None
        if w is None or h is None or not (math.isfinite(w) and math.isfinite(h)):
            return dex.DrawResult.Complete(region=None)
        (x, y) = (base.pos.x, base.pos.y)

        half = max((w - GAP) / 2.0, 1.0)
        ctx.draw_inspectable_node(self.left, at(x, y, half, h))
        ctx.draw_inspectable_node(self.right, at(x + half + GAP, y, half, h))

        ws = ctx.node.workspace
        if self.mirrors:
            (left, right) = (view_of(ws, self.left), view_of(ws, self.right))
            if ws.send_request(left, Selection()) is not None:
                mirror_selection(ws, left, [right])
            elif ws.send_request(right, Selection()) is not None:
                mirror_selection(ws, right, [left])

        # Last, and over both: it reads what the two have just recorded, and
        # reaches through a plane to the view on it the same way this does.
        ctx.draw_node(self.joiner, at(x, y, w, h))
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))


def transform():
    """Join copies of the two views wired into `thisView` and `thatView`."""
    ws = dex.ws
    left = globals().get("thisView")
    right = globals().get("thatView")
    if left is None or right is None:
        raise ValueError("wire a view into each of thisView and thatView")
    # Copies, so the originals keep their place and their ids: see the module
    # docstring. `deep_clone` takes the whole subtree each view owns — a plane
    # and the plot on it and its chrome — and hands back the copy's root.
    (left, right) = (ws.deep_clone(left), ws.deep_clone(right))
    joiner = Correspondence(left, right)
    # Every line at once, so the join is plain to see; set this True once the
    # two are dense enough that all of them together read as a grey haze.
    joiner.only_selected = False
    return ws.insert_node_dyn(Connected(left, right, ws.insert_node_dyn(joiner)))
