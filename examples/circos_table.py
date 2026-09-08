"""A circular phylogeny driven by a Table, iTOL style.

`circos3.py` reads its tree from a column of lineage strings. This is the same
picture built from the *other* shape a tree arrives in, and the one a
phylogenetics tool actually emits: one row per node, a `parent` column giving
the edges, a `depth` column carrying cumulative branch length, and a
`leaf_order` column fixing where the tips fall.

That shape is the prelude's `Tree.from_edges`, reached by naming a `parent`
column when the tree is built. Everything else is identical to `circos3.py` —
the same `Phylogeny` node, the same `AnnotationRings` idea — which is the point:
two quite different ways of writing a tree down produce the same object, so the
picking, the readout and the whole query protocol are written once.

Branch lengths are to scale because `depth` is a distance, not a hop count: a
node's radius is its cumulative distance from the root. Drop the `depth` column
and the tree still draws, with depth counted in hops instead.

Going outward from the middle: the tree, then one ring per annotation column,
then the clade names. Rings are placed by *asking the tree where it drew each
row*, so nothing here can fall out of step with the topology inside it.

Wire a `Table` with that shape into `thisData`; with nothing wired it builds a
small one, so it opens onto a picture.
"""

import math

PLOT_SIZE = (1200.0, 1200.0)

#: Which column plays which part. Named once, so the same transform draws any
#: table of this shape.
NODE_COL = "node"
PARENT_COL = "parent"
DEPTH_COL = "depth"
LEAF_ORDER_COL = "leaf_order"
CLADE_COL = "phylum"

#: Columns that describe the tree rather than annotate it, so they are never
#: drawn as rings.
STRUCTURAL = (NODE_COL, PARENT_COL, DEPTH_COL, LEAF_ORDER_COL, "is_leaf", "label")

BAND_W = 20.0
BAND_GAP = 12.0


def sample():
    """A small tree in edge-list form, with two annotations per tip."""
    import random

    rng = random.Random(17)
    clades = ["Bacillota", "Pseudomonadota", "Actinomycetota"]
    rows = {NODE_COL: [], PARENT_COL: [], DEPTH_COL: [],
            LEAF_ORDER_COL: [], CLADE_COL: [], "label": [],
            "genome_mb": [], "habitat": []}

    def add(node, parent, depth, order, clade, label, size, habitat):
        rows[NODE_COL].append(node)
        rows[PARENT_COL].append(parent)
        rows[DEPTH_COL].append(depth)
        rows[LEAF_ORDER_COL].append(order)
        rows[CLADE_COL].append(clade)
        rows["label"].append(label)
        rows["genome_mb"].append(size)
        rows["habitat"].append(habitat)

    add("root", None, 0.0, None, None, "root", None, None)
    tip = 0
    for (c, clade) in enumerate(clades):
        add(clade, "root", 0.3, None, clade, clade, None, None)
        for g in range(4):
            genus = "%s_g%d" % (clade, g)
            add(genus, clade, 0.6, None, clade, genus, None, None)
            for s in range(3):
                add("%s_s%d" % (genus, s), genus,
                    round(rng.uniform(0.9, 1.4), 3), tip, clade,
                    "%s sp%d" % (genus, s),
                    round(rng.uniform(1.8, 7.2), 2),
                    rng.choice(["soil", "gut", "marine"]))
                tip += 1
    return rows


class AnnotationRings:
    """Bands of per-tip annotation outside a circular tree.

    Each band is one column: a colour per level for a categorical one, a bar for
    a continuous one. Placed by asking the tree where it drew each row, so the
    rings follow whatever the tree did — at any zoom, and after any change to
    how it lays itself out.
    """

    def __init__(self, tree, columns):
        self.tree = tree
        self.columns = list(columns)

    def owned_nodes(self):
        return []

    def type_name(self):
        return "Annotation Rings"

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        h = base.y.provided_value() if base.y is not None else None
        if w is None or h is None or not self.columns:
            return dex.DrawResult.Complete(region=None)
        ws = ctx.node.workspace
        drawn = ws.send_request(self.tree, DrawnPoints()) or {}
        table = ws.send_request(self.tree, SourceTable())
        if not drawn or table is None:
            return dex.DrawResult.Complete(region=None)

        # Only the tips carry annotations, and only they should get a block:
        # an internal node has no value of its own to show.
        tips = {row: to_local(ctx, p) for (row, p) in drawn.items()
                if is_num(table.column(LEAF_ORDER_COL).to_pylist()[row])} \
            if LEAF_ORDER_COL in table.column_names else \
            {row: to_local(ctx, p) for (row, p) in drawn.items()}
        if not tips:
            return dex.DrawResult.Complete(region=None)

        points = list(tips.values())
        cx = sum(p[0] for p in points) / len(points)
        cy = sum(p[1] for p in points) / len(points)
        reach = max(math.hypot(p[0] - cx, p[1] - cy) for p in points)
        if reach <= 1.0:
            return dex.DrawResult.Complete(region=None)
        width = 2.0 * math.pi / max(len(tips), 1)

        for (band, column) in enumerate(self.columns):
            r0 = reach + BAND_GAP + band * BAND_W
            self.paint_band(ctx, table, tips, column, cx, cy, r0, width)
            (cw, _ch) = measure(ctx, column, TICK_FONT)
            text(ctx, column, cx - cw / 2.0, cy - r0 - BAND_W, TICK_FONT, FAINT)
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))

    def paint_band(self, ctx, table, tips, column, cx, cy, r0, width):
        values = table.column(column).to_pylist()
        numeric = [v for v in values if is_num(v)]
        levels = []
        if not numeric:
            for v in values:
                if v is not None and v not in levels:
                    levels.append(v)
        palette = spread_palette(len(levels)) if levels else []
        (lo, hi) = (min(numeric), max(numeric)) if numeric else (0.0, 1.0)
        span = (hi - lo) or 1.0

        for (row, (px, py)) in tips.items():
            if row >= len(values):
                continue
            value = values[row]
            angle = math.atan2(py - cy, px - cx)
            if numeric:
                if not is_num(value):
                    continue
                (ink, r1) = (POINT, r0 + (BAND_W - 3.0) * (float(value) - lo) / span)
            else:
                if value not in levels:
                    continue
                (ink, r1) = (palette[levels.index(value)], r0 + BAND_W - 3.0)
            polygon(ctx, sector_points(cx, cy, r0, r1,
                                       angle - width / 2.0, angle + width / 2.0), ink)


def annotation_columns(frame):
    """The columns worth drawing as rings: everything that is not structure."""
    return [c for c in frame.columns if c not in STRUCTURAL][:4]


def transform():
    """The circular tree of `thisData`'s edge list, ringed with its annotations."""
    ws = dex.ws
    frame = Frame(globals().get("thisData") or sample())
    if NODE_COL not in frame.columns or PARENT_COL not in frame.columns:
        raise ValueError(
            "this wants one row per node, with %r and %r columns" % (NODE_COL, PARENT_COL))

    tree = build_plot(
        ws, Phylogeny,
        frame=frame,
        x=NODE_COL,
        parent=PARENT_COL,
        depth=DEPTH_COL,
        leaf_order=LEAF_ORDER_COL,
        color=CLADE_COL,
        shape="circular",
    )
    tree.chrome = False
    body = ws.insert_node_dyn(tree)
    rings = ws.insert_node_dyn(AnnotationRings(body, annotation_columns(frame)))
    readout = ws.insert_node_dyn(PlotChrome(body))
    return on_plane(ws, body, PLOT_SIZE, "Phylogeny", [rings, readout])
