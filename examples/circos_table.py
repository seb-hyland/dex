"""A circular phylogeny driven by a Table, iTOL style.

`circos3.py` reads its tree from a Newick string and carries its annotations as
hardcoded Python lists. This is the same picture — a tree in the middle, rings
of per-tip annotation around the outside — built instead from a **Table wired
into the transform**, which is the shape real data arrives in: one row per node,
a `parent` column giving the edges, and a column per annotation.

Because the roles of the columns are named once at the top and everything else
is derived, the same transform draws any table with that shape. Point it at a
different tree with different annotation columns and it follows them: each
annotation column becomes a ring, binary columns draw as presence blocks and
continuous ones as a heat ramp, and a chosen categorical column becomes the
outer labelled clade ring and tints the subtrees.

Going outward from the middle:

  * **The tree** — every node at a radius set by its `depth` (its cumulative
    distance from the root, so branch lengths are to scale) and every tip at an
    angle set by its `leaf_order`. A node draws as a radial line out to each
    child plus an arc spanning them; that pair is what makes it read as a tree
    rather than a starburst. A subtree that falls entirely within one clade
    takes that clade's colour, so it reads as one; the backbone above the
    clades belongs to none and stays plain.
  * **The annotation rings** — one per annotation column, each a well with a
    per-tip block. A metal that a genome binds, a process it takes part in: the
    block is present where the value is, absent where it is not.
  * **The clade ring** — the outermost, one arc per run of the categorical
    column, labelled where there is room.

Every radius is a *fraction* of the plot's own, so the whole thing scales with
its box. The one thing measured in points is the labels, and those are dropped
when the plot is too small to seat them — a ring that cannot be labelled is
still worth drawing.

Unlike circos3 the branches are *painted*, not built as one node each: a GTDB
tree runs to thousands of tips, and a node per branch would be tens of
thousands of nodes. The trade is that a single branch is not clickable. For a
small tree where that matters, build the branches as nodes the way circos3 does
— the geometry here is the same, only the sink differs.

**Scale.** A GTDB tree is tens of thousands of nodes, so the plot is built to
survive that. The layout and every polygon are computed once and cached, keyed
by the box's size, then only replayed frame to frame — a resize rebuilds, a
move does not. Each ring draws one block per *run* of like-valued tips rather
than one per tip (continuous values are quantised so they merge too), so a ring
is a few dozen polygons, not ten thousand. And a clade is labelled only where
its arc is wide enough to seat the name, so the rim is not buried under fifty
overlapping phylum labels. See `TablePhylogeny` for the details.

Wire a Table into this lambda (the transform finds it whatever the parameter is
named) and the plot follows it.
"""

import math
import sys
import time

# Set True to print a per-phase timing breakdown to stderr and draw a small
# readout in the corner of the plot. The one-time build cost and the per-frame
# replay cost are reported separately, because they are different problems: a
# slow build is a slow *resize*, a slow replay is a slow *everything*.
PROFILE = True

# ======================================================================
# Column roles — the only thing to change for a differently-shaped table
# ======================================================================

# The structural columns: the edge list and where each node sits.
NODE_COL = "node"            # this node's id
PARENT_COL = "parent"        # its parent's id; empty on the root
DEPTH_COL = "depth"          # cumulative distance from the root -> radius
LEAF_COL = "is_leaf"         # 1 on a tip
LEAF_ORDER_COL = "leaf_order"  # a tip's position around the ring
LABEL_COL = "label"          # taxon name, for reference

# The categorical column drawn as the outer labelled ring and used to tint the
# subtrees beneath it. Set to None to skip the clade ring entirely.
CLADE_COL = "phylum"

# Columns that are neither structural nor an annotation ring: carried in the
# table but not drawn.
META_COLS = {"key", "kind", "n_contigs", "domain"}

# An annotation column is recognised by a ":" in its name — "Zn:binds",
# "bioleaching:process". The part before the ":" labels its ring; the part
# after groups columns into families that are kept together and share a hue.
# A table without ":" in its column names can instead list them explicitly:
# set ANNOTATION_COLS to a list and it is used verbatim.
ANNOTATION_COLS = None


def annotation_columns(names):
    """The annotation columns of `names`, grouped and ordered by family.

    Kept out of the structural and metadata roles, so a column added to the
    table shows up as a ring without any change here.
    """
    if ANNOTATION_COLS is not None:
        chosen = [n for n in ANNOTATION_COLS if n in names]
    else:
        structural = {
            NODE_COL, PARENT_COL, DEPTH_COL, LEAF_COL, LEAF_ORDER_COL,
            LABEL_COL, CLADE_COL, "distance", "phylum",
        }
        chosen = [
            n for n in names
            if ":" in n and n not in structural and n not in META_COLS
        ]
    # Group by the family after the ":", families in first-seen order, columns
    # within a family in table order. Innermost ring first (nearest the tree).
    families = []
    grouped = {}
    for name in chosen:
        family = name.split(":", 1)[1] if ":" in name else ""
        if family not in grouped:
            grouped[family] = []
            families.append(family)
        grouped[family].append(name)
    return [name for family in families for name in grouped[family]], grouped, families


# ======================================================================
# Geometry
# ======================================================================

# A wedge left open at the top, so the first and last tips do not collide and
# the ring labels have somewhere to sit. Widened from the first pass so the long
# process names have horizontal room and do not run into the plot.
OPEN_ANGLE = 0.36
START_ANGLE = -math.pi / 2 + OPEN_ANGLE / 2.0

# Every radius is a *fraction* of the plot's own, outermost first. The tree is
# pulled in and the annotation stack widened relative to the first pass: with
# eighteen rings, a thin stack leaves each ring shorter than its own label is
# tall, which is what made the names collide. A thicker ring is a taller label
# slot.
R_CLADE_OUT = 1.00
R_CLADE_IN = 0.955
R_ANNO_OUT = 0.945          # outer edge of the annotation stack
R_ANNO_IN = 0.50            # inner edge; the tree lives below this
R_LEAF = 0.48               # the tip ring
R_ROOT = 0.04

# A hair off each end of a per-tip block, as a fraction of the tip spacing, so
# neighbouring blocks read as separate cells.
BLOCK_INSET = 0.08
# A hair off the top and bottom of each ring, so the rings read as separate
# tracks rather than one wide band.
RING_GAP = 0.15

# The most a flattened curve may bow from the true one, in points. Sampling
# against this rather than a fixed chord keeps every arc equally smooth: the
# error a straight segment makes is its sagitta, which grows with the radius.
CURVE_TOLERANCE = 0.12

BRANCH_WIDTH_AT = 420.0     # branch weight scales with the plot
BRANCH_WIDTH = 1.1

CLADE_FONT = 11.0
RING_FONT = 8.5
CLADE_LABEL_GAP = 7.0
RING_LABEL_GAP = 4.0
PADDING = 8.0
# Below this radius labels are dropped rather than reserved for: a band still
# reads without one, and reserving room for text that will not fit is what
# stops a plot working when it shrinks.
LABEL_MIN_RADIUS = 170.0
# A clade is labelled only when its arc is at least this many points long — the
# rim of a GTDB tree holds fifty-odd phyla and most are slivers, so labelling
# every one buries the plot in overlapping text. The sliver still gets its
# coloured arc; it just goes unnamed until the plot is large enough to seat it.
MIN_CLADE_LABEL_ARC = 26.0

# A continuous column's values are quantised to this many levels before drawing,
# which is what lets neighbouring tips with near-equal values merge into one
# block instead of one polygon each. The ramp still reads continuous at twenty
# steps, and the shape count collapses.
RAMP_LEVELS = 20

INK = (58, 62, 70)
BRANCH_INK = (150, 156, 166)
TRACK_BG = (240, 242, 246)


def arc_steps(radius, sweep):
    """How many samples an arc of `sweep` radians at `radius` deserves."""
    if radius <= 0.0:
        return 2
    theta = math.sqrt(8.0 * CURVE_TOLERANCE / radius)
    return max(2, int(abs(sweep) / theta) + 2)


def polar(cx, cy, radius, angle):
    return (cx + radius * math.cos(angle), cy + radius * math.sin(angle))


def arc(cx, cy, radius, a0, a1):
    """Points along the arc from `a0` to `a1`, inclusive of both ends."""
    steps = arc_steps(radius, a1 - a0)
    return [
        polar(cx, cy, radius, a0 + (a1 - a0) * i / (steps - 1)) for i in range(steps)
    ]


def sector(cx, cy, r_in, r_out, a0, a1):
    """An annulus sector: out along one arc, back along the other."""
    return arc(cx, cy, r_out, a0, a1) + arc(cx, cy, r_in, a1, a0)


def sectors(cx, cy, r_in, r_out, a0, a1):
    """A filled band, split into as many polygons as it takes to fill cleanly.

    Ear-clipping a thin wide sector bridges the hole in the middle — the chord
    between the ends of the outer arc dips below the inner one — leaving a white
    wedge. Cutting the sweep until the chord stays inside the band prevents it.
    """
    thickness = r_out - r_in
    if thickness <= 0.0 or r_out <= 0.0:
        return []
    limit = 1.0 - max(min(0.25 * thickness / r_out, 1.0), 0.0)
    widest = 2.0 * math.acos(max(-1.0, min(limit, 1.0)))
    if widest <= 0.0:
        widest = math.pi / 6.0
    sweep = a1 - a0
    parts = max(1, int(math.ceil(abs(sweep) / widest)))
    step = sweep / parts
    return [
        sector(cx, cy, r_in, r_out, a0 + step * i, a0 + step * (i + 1))
        for i in range(parts)
    ]


def lerp_rgb(a, b, t):
    """The colour `t` of the way from `a` to `b`."""
    t = max(0.0, min(t, 1.0))
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def hsv_rgb(h, s, v):
    """An RGB triple from HSV, for generating a palette when none is given."""
    i = int(h * 6.0)
    f = h * 6.0 - i
    p, q, t = v * (1 - s), v * (1 - s * f), v * (1 - s * (1 - f))
    r, g, b = [
        (v, t, p), (q, v, p), (p, v, t), (p, q, v), (t, p, v), (v, p, q)
    ][i % 6]
    return (int(r * 255), int(g * 255), int(b * 255))


# ======================================================================
# The table -> a tree
# ======================================================================


def as_int(x):
    """`x` as an int, or None when it is blank."""
    if x is None or x == "":
        return None
    try:
        return int(x)
    except (TypeError, ValueError):
        try:
            return int(float(x))
        except (TypeError, ValueError):
            return None


def as_float(x):
    if x is None or x == "":
        return None
    try:
        return float(x)
    except (TypeError, ValueError):
        return None


class Tree:
    """The forest of nodes the table describes, laid out for drawing.

    Built once from the columns and then only read, so it can be rebuilt each
    draw from the picklable column data the node stores — the same reason
    circos3 stores its Newick string rather than a tree of objects.
    """

    def __init__(self, columns):
        self.columns = columns
        names = list(columns.keys())
        node_ids = [as_int(v) for v in columns[NODE_COL]]
        parents = [as_int(v) for v in columns[PARENT_COL]]
        depths = [as_float(v) or 0.0 for v in columns[DEPTH_COL]]
        is_leaf = [bool(as_int(v)) for v in columns[LEAF_COL]]
        leaf_order = [as_int(v) for v in columns[LEAF_ORDER_COL]]

        self.row_of = {nid: i for (i, nid) in enumerate(node_ids) if nid is not None}
        self.children = {nid: [] for nid in self.row_of}
        self.parent = {}
        self.root = None
        for (i, nid) in enumerate(node_ids):
            if nid is None:
                continue
            p = parents[i]
            self.parent[nid] = p
            if p is None or p not in self.row_of:
                self.root = nid
            else:
                self.children[p].append(nid)

        self.depth = {nid: depths[self.row_of[nid]] for nid in self.row_of}
        self.is_leaf = {nid: is_leaf[self.row_of[nid]] for nid in self.row_of}
        self.max_depth = max(self.depth.values()) if self.depth else 1.0

        # Tips in ring order, and the angle each sits at.
        self.tips = sorted(
            (nid for nid in self.row_of if self.is_leaf[nid]),
            key=lambda nid: (
                leaf_order[self.row_of[nid]]
                if leaf_order[self.row_of[nid]] is not None
                else self.row_of[nid]
            ),
        )
        self.angle = {}
        n = len(self.tips)
        step = (2.0 * math.pi - OPEN_ANGLE) / max(n - 1, 1)
        self.step = step
        for (i, tip) in enumerate(self.tips):
            self.angle[tip] = START_ANGLE + step * i
        self._resolve_angles(self.root)

        # The clade a node sits in: the one value of `CLADE_COL` among the tips
        # beneath it, or None where they disagree (the backbone) or the column
        # is absent.
        self.clade = {}
        self.clade_of_tip = {}
        if CLADE_COL and CLADE_COL in columns:
            clade_col = columns[CLADE_COL]
            for tip in self.tips:
                self.clade_of_tip[tip] = clade_col[self.row_of[tip]]
            self._resolve_clades(self.root)

    def _resolve_angles(self, nid):
        """An internal node sits at the midpoint of its children."""
        if nid is None:
            return 0.0
        kids = self.children.get(nid, [])
        if not kids:
            return self.angle.get(nid, 0.0)
        angles = [self._resolve_angles(k) for k in kids]
        self.angle[nid] = sum(angles) / len(angles)
        return self.angle[nid]

    def _resolve_clades(self, nid):
        """The set of tip clades beneath `nid`; records the singleton ones."""
        if nid is None:
            return set()
        if self.is_leaf.get(nid):
            here = {self.clade_of_tip.get(nid)}
            self.clade[nid] = self.clade_of_tip.get(nid)
            return here
        seen = set()
        for k in self.children.get(nid, []):
            seen |= self._resolve_clades(k)
        self.clade[nid] = next(iter(seen)) if len(seen) == 1 else None
        return seen

    def radius_frac(self, nid):
        """Where `nid` sits between the root and the tip ring, in [0, 1]."""
        t = self.depth[nid] / self.max_depth if self.max_depth > 0 else 0.0
        return R_ROOT + (R_LEAF - R_ROOT) * t

    def walk(self):
        yield from self.row_of

    def clade_runs(self):
        """Contiguous runs of one clade among the tips: `(clade, a0, a1)`.

        Per run rather than per clade value, so a clade that is not
        monophyletic draws as the several arcs it actually occupies instead of
        one arc bridging the gap.
        """
        runs = []
        start = None
        for (i, tip) in enumerate(self.tips):
            c = self.clade_of_tip.get(tip)
            if start is None or c != start[0]:
                if start is not None:
                    runs.append(start)
                start = [c, self.angle[tip], self.angle[tip]]
            else:
                start[2] = self.angle[tip]
        if start is not None:
            runs.append(start)
        return [(c, a0, a1) for (c, a0, a1) in runs if c not in (None, "")]


# ======================================================================
# Annotation columns -> ring specs
# ======================================================================


def column_kind(values):
    """`("binary", None)` or `("ramp", (lo, hi))` for a column's values."""
    nums = [v for v in (as_float(x) for x in values) if v is not None]
    if not nums:
        return ("binary", None)
    lo, hi = min(nums), max(nums)
    if all(v in (0.0, 1.0) for v in nums):
        return ("binary", None)
    return ("ramp", (lo, hi))


def ring_palette(columns, grouped, families):
    """A colour per annotation column, hues by family, shades within it.

    A generated palette so the plot works with no configuration; swap in your
    own `{column: (r, g, b)}` if you have one.
    """
    out = {}
    for (fi, family) in enumerate(families):
        base_hue = (fi / max(len(families), 1)) * 0.85
        cols = grouped[family]
        for (ci, name) in enumerate(cols):
            spread = 0.10 * (ci / max(len(cols) - 1, 1) - 0.5) if len(cols) > 1 else 0.0
            out[name] = hsv_rgb((base_hue + spread) % 1.0, 0.55, 0.72)
    return out


# ======================================================================
# The node
# ======================================================================


class TablePhylogeny:
    """A circular phylogeny built from a table's columns.

    Stores the columns (picklable primitives) rather than the tree, and rebuilds
    the tree — and the whole plot's geometry — only when the box it is given
    changes size. A GTDB tree is tens of thousands of nodes; recomputing the
    layout and remaking every polygon on every frame is what turns a resize into
    a freeze, so the built geometry is cached and only *replayed* frame to frame.

    The geometry is held **relative to the centre** and drawn through a centre
    origin, so moving the plot around the canvas reuses the cache — only a
    resize invalidates it.

    Two things keep the polygon count survivable at GTDB scale:

      * **Runs, not cells.** A binary ring draws one block per *run* of present
        tips, not one per tip, and skips the absent ones entirely. A continuous
        ring quantises its values (see `RAMP_LEVELS`) so neighbouring tips at the
        same level merge into one block too. A ring of ten thousand tips becomes
        a few dozen polygons.
      * **One background, not eighteen.** The empty track behind every ring is a
        single annulus with the ring blocks drawn over it, rather than a
        separately-filled well per column.
    """

    def __init__(self, columns):
        self.columns = {k: list(v) for (k, v) in columns.items()}
        names = list(self.columns.keys())
        self.anno_cols, self.grouped, self.families = annotation_columns(names)
        self.kinds = {n: column_kind(self.columns[n]) for n in self.anno_cols}
        self.ring_ink = ring_palette(self.columns, self.grouped, self.families)
        # The clade palette: a colour per distinct value of the clade column.
        self.clade_ink = {}
        if CLADE_COL and CLADE_COL in self.columns:
            vals = []
            for v in self.columns[CLADE_COL]:
                if v not in (None, "") and v not in vals:
                    vals.append(v)
            for (i, v) in enumerate(vals):
                self.clade_ink[v] = hsv_rgb((i / max(len(vals), 1)) % 1.0, 0.42, 0.74)
        # The cache: the box it was built for, the shapes (centre-relative), and
        # the labels (centre-relative offsets, already measured).
        self._built = None       # (width, height)
        self._paths = []         # [dex.Path] drawn through the centre origin
        self._labels = []        # [(dex.Label, ox, oy)]
        # Profiling: the last build's phase breakdown, and a running mean of the
        # per-frame replay cost.
        self._stats = None       # dict of counts + build timings
        self._replay_ms = 0.0
        self._frames = 0

    # -- drawing ---------------------------------------------------------

    def draw(self, ctx):
        base = ctx.constraints
        width = base.x.provided_value() if base.x is not None else None
        height = base.y.provided_value() if base.y is not None else None
        if width is None or height is None:
            return dex.DrawResult.Complete(region=None)

        radius = min(width, height) / 2.0 - PADDING
        if radius <= 0.0:
            return dex.DrawResult.Complete(region=None)

        if self._built != (width, height):
            self._build(ctx, radius)
            self._built = (width, height)

        cx = base.pos.x + width / 2.0
        cy = base.pos.y + height / 2.0
        origin = self._origin(cx, cy)
        # The steady-state cost: everything here runs every frame. Timed on its
        # own, split from label drawing, because the paths are where the count
        # is and so where any per-frame slowness lives.
        t0 = time.perf_counter()
        for path in self._paths:
            ctx.draw_node(path, origin)
        paths_ms = (time.perf_counter() - t0) * 1e3

        t1 = time.perf_counter()
        for (label, ox, oy) in self._labels:
            ctx.draw_node(
                label,
                dex.DrawConstraints(
                    pos=dex.ScreenPos.new(cx + ox, cy + oy),
                    x=None, y=None, wrap=None, should_clip=False,
                ),
            )
        labels_ms = (time.perf_counter() - t1) * 1e3

        if PROFILE:
            self._report_frame(ctx, base, paths_ms, labels_ms)

        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(
                base.pos, dex.Vector.new(width, height)
            )
        )

    def _report_frame(self, ctx, base, paths_ms, labels_ms):
        """Accumulate replay timings, draw a corner readout, log every 60th."""
        self._frames += 1
        self._replay_ms += paths_ms + labels_ms
        avg = self._replay_ms / self._frames
        s = self._stats or {}
        text = (
            "build %.0fms (tree %.0fms) | replay %.1fms (paths %.1f + labels %.1f, mean %.1f)"
            " | %d paths = %d branch + %d ring + %d clade | %d tips"
            % (
                s.get("build_ms", 0.0), s.get("tree_ms", 0.0),
                paths_ms + labels_ms, paths_ms, labels_ms, avg,
                s.get("total", 0), s.get("branches", 0), s.get("rings", 0),
                s.get("clades", 0), s.get("tips", 0),
            )
        )
        label = dex.Label.new(text)
        label.font = dex.Font.proportional(11.0)
        label.color = dex.Color.rgb(150, 96, 96)
        ctx.draw_node(
            label,
            dex.DrawConstraints(
                pos=dex.ScreenPos.new(base.pos.x + 8.0, base.pos.y + 8.0),
                x=None, y=None, wrap=None, should_clip=False,
            ),
        )
        if self._frames % 60 == 0:
            sys.stderr.write("[circos] " + text + "\n")
            sys.stderr.flush()

    def _origin(self, cx, cy):
        """Constraints placing a path's own (centre-relative) coords at the centre."""
        return dex.DrawConstraints(
            pos=dex.ScreenPos.new(cx, cy),
            x=None, y=None, wrap=None, should_clip=False,
        )

    # -- building (once per size) ---------------------------------------

    def _build(self, ctx, radius):
        """Lay the whole plot out once, into the path and label caches.

        Centre-relative, so the same shapes redraw wherever the plot sits. Each
        phase is timed and its share of the path count recorded, so a slow build
        says *which* phase — tree layout, branches, rings, or clades — to blame.
        """
        self._paths = []
        self._labels = []
        self._stats = None
        self._replay_ms = 0.0
        self._frames = 0

        t0 = time.perf_counter()
        tree = Tree(self.columns)
        t_tree = time.perf_counter()
        if not tree.tips or tree.root is None:
            return
        named = radius >= LABEL_MIN_RADIUS

        n0 = len(self._paths)
        self._build_branches(radius, tree)
        n_branch = len(self._paths)
        self._build_rings(ctx, radius, tree, named)
        n_ring = len(self._paths)
        self._build_clades(ctx, radius, tree, named)
        n_clade = len(self._paths)
        t_end = time.perf_counter()

        self._stats = {
            "tips": len(tree.tips),
            "nodes": len(tree.row_of),
            "branches": n_branch - n0,
            "rings": n_ring - n_branch,
            "clades": n_clade - n_ring,
            "total": n_clade,
            "tree_ms": (t_tree - t0) * 1e3,
            "build_ms": (t_end - t0) * 1e3,
        }
        if PROFILE:
            s = self._stats
            sys.stderr.write(
                "[circos] BUILD %.0fms | tree %.0fms, branches %.0fms-ish | "
                "%d paths = %d branch + %d ring + %d clade | %d tips, %d nodes\n"
                % (
                    s["build_ms"], s["tree_ms"],
                    s["build_ms"] - s["tree_ms"],
                    s["total"], s["branches"], s["rings"], s["clades"],
                    s["tips"], s["nodes"],
                )
            )
            sys.stderr.flush()

    def _polygon(self, points, rgb, alpha=255):
        if len(points) < 3:
            return
        self._paths.append(
            dex.Path.polygon(
                [dex.Vector.new(x, y) for (x, y) in points],
                dex.Color.rgba(rgb[0], rgb[1], rgb[2], alpha),
                dex.Stroke.none(),
            )
        )

    def _stroke(self, points, rgb, width):
        if len(points) < 2:
            return
        self._paths.append(
            dex.Path.polyline(
                [dex.Vector.new(x, y) for (x, y) in points],
                dex.Stroke.new(width, dex.Color.rgb(*rgb)),
            )
        )

    def _label(self, ctx, text, r, angle, font, wrap, centre=False):
        """Cache a label at radius `r`, angle `angle` (offsets from the centre)."""
        metrics = ctx.measure_text(text, font, wrap)
        (x, y) = polar(0.0, 0.0, r, angle)
        if centre:
            # Pushed out along its own radius so it clears the ring.
            x += math.cos(angle) * metrics.width / 2.0
            y += math.sin(angle) * metrics.height / 2.0
            ox, oy = x - metrics.width / 2.0, y - metrics.height / 2.0
        else:
            ox, oy = x - metrics.width, y - metrics.height / 2.0
        label = dex.Label.new(text)
        label.font = font
        label.color = dex.Color.rgb(*INK)
        self._labels.append((label, ox, oy))

    def _build_branches(self, radius, tree):
        """Every branch as a painted polyline, tinted by its clade.

        An internal node contributes the arc spanning its children plus a radial
        line out to each. The arc carries the topology — without it the tree is
        a starburst. Painted, not built as nodes: a GTDB tree is too many
        branches to make one workspace node each, and per-branch editing is not
        wanted here.

        Where a node and all its children share one colour — anywhere inside a
        clade, which is most of the tree — the arc and its spokes are emitted as
        **one** polyline that traces the arc and retraces each spoke, rather than
        one stroke per branch. It draws the identical pixels (a retraced spoke
        overpaints itself) at roughly a third of the draw calls, and the draw
        calls are what the frame time is made of. Only at a clade boundary, where
        the spokes want different colours from the arc, does a node fall back to
        a stroke apiece.
        """
        weight = max(0.6, BRANCH_WIDTH * radius / BRANCH_WIDTH_AT)
        for nid in tree.walk():
            kids = tree.children.get(nid, [])
            if not kids:
                continue
            r = radius * tree.radius_frac(nid)
            node_ink = self.clade_ink.get(tree.clade.get(nid), BRANCH_INK)
            # Children left to right, so the arc between neighbours is short.
            skids = sorted(kids, key=lambda k: tree.angle[k])
            inks = [self.clade_ink.get(tree.clade.get(k), BRANCH_INK) for k in skids]

            if all(ci == node_ink for ci in inks):
                pts = []
                for (idx, k) in enumerate(skids):
                    a = tree.angle[k]
                    rk = radius * tree.radius_frac(k)
                    if idx > 0:
                        pts.extend(arc(0.0, 0.0, r, tree.angle[skids[idx - 1]], a))
                    else:
                        pts.append(polar(0.0, 0.0, r, a))
                    pts.append(polar(0.0, 0.0, rk, a))   # out along the spoke
                    pts.append(polar(0.0, 0.0, r, a))    # and back to the arc
                self._stroke(pts, node_ink, weight)
                continue

            angles = [tree.angle[k] for k in skids]
            self._stroke(arc(0.0, 0.0, r, min(angles), max(angles)), node_ink, weight)
            for (k, k_ink) in zip(skids, inks):
                self._stroke(
                    [
                        polar(0.0, 0.0, r, tree.angle[k]),
                        polar(0.0, 0.0, radius * tree.radius_frac(k), tree.angle[k]),
                    ],
                    k_ink, weight,
                )

    def _ring_runs(self, tree, name):
        """`(a0, a1, rgb)` per drawable run of a ring — merged, absent skipped.

        This is the whole performance story: a run of like-valued tips is one
        block, not one polygon each, so a ten-thousand-tip ring costs a few
        dozen polygons instead of ten thousand.
        """
        kind, span = self.kinds[name]
        ink = self.ring_ink[name]
        col = self.columns[name]
        half = tree.step * (0.5 - BLOCK_INSET)

        def level_colour(v):
            if kind == "binary":
                return ink if v > 0.0 else None
            lo, hi = span
            if hi <= lo:
                return None
            q = round((v - lo) / (hi - lo) * RAMP_LEVELS)
            return lerp_rgb(TRACK_BG, ink, q / RAMP_LEVELS)

        runs = []
        run = None  # [rgb, first_angle, last_angle]
        for tip in tree.tips:
            v = as_float(col[tree.row_of[tip]])
            rgb = None if v is None else level_colour(v)
            if rgb is None:
                if run is not None:
                    runs.append(run)
                    run = None
                continue
            if run is not None and run[0] == rgb:
                run[2] = tree.angle[tip]
            else:
                if run is not None:
                    runs.append(run)
                run = [rgb, tree.angle[tip], tree.angle[tip]]
        if run is not None:
            runs.append(run)
        return [(a0 - half, a1 + half, rgb) for (rgb, a0, a1) in runs]

    def _build_rings(self, ctx, radius, tree, named):
        """One ring per annotation column, over a single shared background."""
        n = len(self.anno_cols)
        if n == 0:
            return
        thickness = (R_ANNO_OUT - R_ANNO_IN) / n
        font = dex.Font.proportional(RING_FONT)
        wrap = dex.TextWrap.singleline()

        # One background annulus behind the whole stack, drawn first.
        a_lo = START_ANGLE - OPEN_ANGLE / 2.0
        a_hi = START_ANGLE + 2.0 * math.pi - 1.5 * OPEN_ANGLE
        for part in sectors(
            0.0, 0.0, radius * R_ANNO_IN, radius * R_ANNO_OUT, a_lo, a_hi
        ):
            self._polygon(part, TRACK_BG)

        specs = []
        for (i, name) in enumerate(self.anno_cols):
            r_lo = R_ANNO_IN + thickness * i
            r_in = radius * (r_lo + thickness * RING_GAP)
            r_out = radius * (r_lo + thickness * (1.0 - RING_GAP))
            for (b0, b1, rgb) in self._ring_runs(tree, name):
                for part in sectors(0.0, 0.0, r_in, r_out, b0, b1):
                    self._polygon(part, rgb)
            specs.append(((r_in + r_out) / 2.0, name.split(":", 1)[0]))
        if named:
            self._place_ring_labels(ctx, specs, font, wrap)

    def _place_ring_labels(self, ctx, specs, font, wrap):
        """Stack the ring names up the open wedge without letting them collide.

        Each name wants to sit straight up at its ring's radius, centred in the
        gap. Where two rings are closer than a line of text is tall, the outer
        one is nudged further out and a thin leader drops back to its ring — so
        the names keep their reading order and every one still points home,
        however tight the stack gets.
        """
        measured = [(r, text, ctx.measure_text(text, font, wrap)) for (r, text) in specs]
        line_h = max((m.height for (_, _, m) in measured), default=0.0) + 3.0
        prev = None
        for (r, text, m) in measured:  # inner ring first, working outward
            natural = -r  # straight up: the ring's own point in the gap
            cy = natural if prev is None else min(natural, prev - line_h)
            prev = cy
            label = dex.Label.new(text)
            label.font = font
            label.color = dex.Color.rgb(*INK)
            self._labels.append((label, -m.width / 2.0, cy - m.height / 2.0))
            if natural - cy > 2.0:  # nudged out; drop a leader back to the ring
                self._stroke([(0.0, cy + m.height / 2.0), (0.0, natural)],
                             (205, 209, 214), 0.8)

    def _build_clades(self, ctx, radius, tree, named):
        """The outer ring: one arc per contiguous run of the clade column.

        Only runs wide enough to seat a name get one — the rim of a real tree is
        mostly slivers, and labelling every one is what buried the plot.
        """
        if not self.clade_ink:
            return
        r_in = radius * R_CLADE_IN
        r_out = radius * R_CLADE_OUT
        font = dex.Font.proportional(CLADE_FONT)
        wrap = dex.TextWrap.singleline()
        pad = tree.step * 0.5
        for (clade, a0, a1) in tree.clade_runs():
            rgb = self.clade_ink.get(clade, BRANCH_INK)
            (b0, b1) = (a0 - pad, a1 + pad)
            for part in sectors(0.0, 0.0, r_in, r_out, b0, b1):
                self._polygon(part, rgb)
            if named and r_out * (b1 - b0) >= MIN_CLADE_LABEL_ARC:
                self._label(ctx, clade, r_out + CLADE_LABEL_GAP,
                            (b0 + b1) / 2.0, font, wrap, centre=True)

    # -- messages --------------------------------------------------------

    def type_name(self):
        return "A Table Phylogeny"

    def build_inspector(self, ctx):
        return None


# ======================================================================
# Transform
# ======================================================================


def _find_table():
    """The Table wired into the lambda, whatever the parameter is named.

    An arg arrives as a global; a Table arrives as a `pyarrow.RecordBatch`. This
    finds it by type, so the parameter can be called `data`, `t`, anything.
    """
    for value in list(globals().values()):
        if type(value).__name__ in ("RecordBatch", "Table"):
            return value
    return None


def _columns(batch):
    """`{name: [values]}` from a pyarrow RecordBatch (or Table)."""
    batch = batch.combine_chunks() if hasattr(batch, "combine_chunks") else batch
    return {name: batch.column(i).to_pylist() for (i, name) in enumerate(batch.column_names)}


def transform():
    """The circular phylogeny of the wired Table."""
    batch = _find_table()
    if batch is None:
        raise ValueError("wire a Table into this transform")
    return TablePhylogeny(_columns(batch))
