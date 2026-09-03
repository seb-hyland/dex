"""A circular phylogeny with annotation bands, iTOL style.

The third circos idiom: the ring is not an axis but the *leaves* of a tree,
and everything inside is the topology that put them in that order. Sixty-four
bacterial isolates, five clades, and five bands of per-tip annotation around
the outside.

The tree is read from Newick, the format every phylogenetics tool emits, so
the string below can be swapped for one out of RAxML or IQ-TREE. Internal
labels are kept, which is what lets a clade name a ring segment.

Going outward from the middle:

  * **The tree** — every node at a radius set by its distance from the root,
    so branch lengths are to scale. A node draws as a radial line out to each
    child plus an arc spanning them; that pair is what makes it read as a tree
    rather than a starburst. At sixty-four tips the names would be unreadable,
    so there are none — the bands carry the per-tip information instead.

    Each of those strokes is its own node in the workspace rather than paint
    this plot lays down, and is drawn through `draw_inspectable_node`. That is
    what makes a single branch clickable: the inspector opens on the path
    itself, and the colour chosen there lives in it. A tree of a few hundred
    nodes costs a few hundred nodes.
  * **Habitat** — a categorical block per tip.
  * **Resistance** — present or absent, drawn only where present.
  * **Genome size** — a bar per tip.
  * **GC content** — a heat block per tip.
  * **Clades** — the outermost ring, one arc per named clade.

Every radius is a *fraction* of the plot's own, so the whole thing scales with
its box: at a canvas-item size the bands thin out but nothing is clipped or
dropped. The one thing measured in points is the clade labels, and those are
skipped entirely when the plot is too small to seat them — a band that cannot
be labelled is still worth drawing, and reserving room for text that will not
fit is what stops a plot working when it shrinks.

Branches are `dex.Path.polyline` — a branch is a line. Bands and blocks are
`dex.Path.polygon` — a band is an area, and those stay plain values, because
there is nothing to say about one cell of a heat ramp on its own.
"""

import math

# ======================================================================
# Data
# ======================================================================

# The tree, in Newick. Sixty-four isolates in five clades, with branch
# lengths — swap in your own and everything below follows it.
NEWICK = """
(
  (((iso001:0.118,iso002:0.049):0.058,((iso003:0.149,iso004:0.066):0.026,(iso005:0.048,iso006:0.051):0.05):0.078):0.029,(((iso007:0.109,(iso008:0.046,iso009:0.067):0.059):0.029,iso010:0.09):0.058,(((iso011:0.052,(iso012:0.085,iso013:0.106):0.024):0.024,(iso014:0.1,iso015:0.104):0.074):0.053,iso016:0.151):0.045):0.037)Bacillota:0.105,
  (((iso017:0.145,iso018:0.128):0.04,iso019:0.158):0.028,(((iso020:0.058,iso021:0.099):0.023,(iso022:0.132,(((iso023:0.123,iso024:0.111):0.061,iso025:0.095):0.079,iso026:0.153):0.053):0.066):0.024,((((iso027:0.126,iso028:0.146):0.044,iso029:0.153):0.045,(iso030:0.099,(iso031:0.132,iso032:0.056):0.037):0.047):0.081,(iso033:0.06,iso034:0.088):0.039):0.03):0.05)Pseudomonadota:0.099,
  (((iso035:0.058,(iso036:0.058,(iso037:0.041,iso038:0.14):0.033):0.04):0.03,(iso039:0.113,iso040:0.078):0.029):0.08,((iso041:0.095,(((iso042:0.088,iso043:0.052):0.064,iso044:0.047):0.025,iso045:0.065):0.031):0.044,iso046:0.046):0.02)Bacteroidota:0.057,
  (((iso047:0.048,(iso048:0.114,(iso049:0.116,(iso050:0.112,iso051:0.097):0.028):0.054):0.088):0.054,iso052:0.077):0.03,(((iso053:0.139,iso054:0.059):0.022,iso055:0.154):0.057,(iso056:0.123,iso057:0.15):0.073):0.041)Actinomycetota:0.056,
  (((iso058:0.083,iso059:0.067):0.058,iso060:0.1):0.065,((iso061:0.137,(iso062:0.129,iso063:0.067):0.056):0.045,iso064:0.043):0.022)Campylobacterota:0.068
)Bacteria;
"""

# The annotation bands, one entry per tip **in the tree's leaf order**. Keyed
# by position rather than by name because that is how an annotation file from
# iTOL or a spreadsheet arrives: a column beside the tip list, not a lookup.
# A band shorter than the tree simply runs out and stops being drawn.

# Where the isolate came from: Gut, Soil, Marine, Clinical.
HABITAT = "SSCSGCGGGGGGGGSGMCSCSMCGMSCCSGGGGMMGGGGGSGSGGGCSCSSCGSCGCGCGCCGM"

# Whether a resistance cassette was found.
RESISTANT = [
    0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 1, 0, 1, 0, 0,
    1, 1, 1, 0, 1, 0, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1,
    0, 1, 1, 0, 0, 1, 1, 1, 0, 0, 0, 0, 1, 0, 0, 0,
    0, 0, 0, 0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1, 1, 1,
]

# Assembled genome size, in megabases.
GENOME_MB = [
    4.43, 4.1, 4.14, 3.51, 3.88, 4.15, 3.63, 3.79, 4.17, 3.73,
    3.78, 4.08, 3.86, 4.51, 3.88, 3.72, 5.1, 4.42, 5.43, 5.89,
    5.13, 6.34, 5.07, 6.38, 5.54, 4.4, 5.01, 5.26, 4.54, 4.62,
    6.29, 4.3, 4.72, 6.19, 4.47, 4.3, 4.99, 3.67, 3.56, 2.96,
    4.4, 5.0, 5.1, 4.5, 4.02, 4.98, 7.79, 6.83, 5.04, 8.18,
    7.35, 6.73, 6.92, 5.36, 5.04, 5.54, 5.77, 1.94, 1.91, 1.71,
    2.36, 1.98, 2.02, 1.66,
]

# GC content, as a percentage.
GC_PERCENT = [
    36.2, 31.5, 31.5, 36.5, 33.6, 36.7, 31.9, 34.7, 34.0, 38.0,
    31.8, 30.9, 37.7, 30.4, 30.6, 30.7, 60.9, 48.8, 54.3, 53.9,
    51.0, 56.3, 60.0, 51.0, 59.6, 56.2, 56.7, 56.0, 60.7, 58.4,
    60.4, 49.5, 57.9, 54.9, 38.6, 42.9, 39.6, 40.0, 42.6, 41.6,
    42.7, 41.8, 38.2, 40.6, 43.9, 40.8, 66.2, 68.2, 62.7, 70.8,
    67.8, 67.2, 66.8, 69.8, 65.0, 71.1, 66.0, 32.8, 31.5, 30.1,
    29.7, 33.6, 32.0, 28.2,
]

# Which named clades get a ring segment, and their colours.
CLADES = {
    "Bacillota": (188, 96, 96),
    "Pseudomonadota": (206, 146, 84),
    "Bacteroidota": (150, 168, 96),
    "Actinomycetota": (96, 158, 156),
    "Campylobacterota": (132, 128, 190),
}

# The habitat codes, and what each is drawn as.
HABITATS = {
    "G": ("Gut", (198, 132, 106)),
    "S": ("Soil", (146, 160, 104)),
    "M": ("Marine", (104, 152, 180)),
    "C": ("Clinical", (176, 118, 168)),
}

RESIST_INK = (196, 92, 92)
GENOME_INK = (118, 134, 160)
# The two ends of the GC heat ramp.
GC_LOW = (232, 236, 242)
GC_HIGH = (72, 96, 132)
GC_RANGE = (26.0, 74.0)
GENOME_MAX = 8.5

# ======================================================================
# Newick
# ======================================================================


class Node:
    """One node of the tree: a name, a branch length, and its children."""

    def __init__(self, name="", length=0.0):
        self.name = name
        self.length = length
        self.children = []
        # Filled in during layout.
        self.angle = 0.0
        self.depth = 0.0

    def is_leaf(self):
        return not self.children

    def leaves(self):
        """Every tip beneath this node, left to right."""
        if self.is_leaf():
            return [self]
        return [leaf for child in self.children for leaf in child.leaves()]

    def walk(self):
        """This node and everything beneath it."""
        yield self
        for child in self.children:
            yield from child.walk()


def tokenize(text):
    """Newick's punctuation, and the runs of text between it."""
    out = []
    buffer = ""
    for ch in text:
        if ch in "(),:;":
            if buffer.strip():
                out.append(buffer.strip())
            buffer = ""
            out.append(ch)
        elif ch in " \t\r\n":
            # Newick ignores whitespace outside names; the tree is indented
            # for reading.
            continue
        else:
            buffer += ch
    if buffer.strip():
        out.append(buffer.strip())
    return out


def parse_newick(text):
    """The tree `text` describes.

    A small recursive-descent reader: enough for the grammar an aligner emits
    — nested parentheses, optional internal labels, optional `:length` — and
    no more. It does not handle quoted names or comments.
    """
    tokens = tokenize(text)
    pos = 0

    def parse_node():
        nonlocal pos
        node = Node()
        if pos < len(tokens) and tokens[pos] == "(":
            pos += 1
            while True:
                node.children.append(parse_node())
                if pos < len(tokens) and tokens[pos] == ",":
                    pos += 1
                    continue
                break
            if pos < len(tokens) and tokens[pos] == ")":
                pos += 1
        # A label here names a leaf, or an internal node after its children.
        if pos < len(tokens) and tokens[pos] not in "(),:;":
            node.name = tokens[pos]
            pos += 1
        if pos < len(tokens) and tokens[pos] == ":":
            pos += 1
            node.length = float(tokens[pos])
            pos += 1
        return node

    return parse_node()


# ======================================================================
# Layout
# ======================================================================

# A wedge left open at the top, so the first and last tips do not collide.
OPEN_ANGLE = 0.22
START_ANGLE = -math.pi / 2 + OPEN_ANGLE / 2.0

# Every radius is a *fraction* of the plot's own, which is what makes the
# whole thing scale with its box: bands thin as it shrinks instead of holding
# a fixed width and squeezing the tree out. Outermost first.
R_CLADE_OUT = 1.00
R_CLADE_IN = 0.955
R_GC_OUT = 0.94
R_GC_IN = 0.895
R_GENOME_OUT = 0.885
R_GENOME_IN = 0.775
R_RESIST_OUT = 0.765
R_RESIST_IN = 0.735
R_HABITAT_OUT = 0.725
R_HABITAT_IN = 0.68
R_LEAF = 0.655
R_ROOT = 0.06

# A hair off each end of a per-tip block, as a fraction of the tip spacing, so
# neighbouring blocks read as separate cells.
BLOCK_INSET = 0.06

# The most a flattened curve may bow away from the true one, in points.
#
# Sampled against this rather than against a fixed chord length, which is what
# makes every arc in the plot equally smooth. The error a straight segment
# introduces is its sagitta, and that grows with the arc's radius — so a fixed
# chord length over-samples the inner rings and under-samples the outer ones.
CURVE_TOLERANCE = 0.12


def arc_steps(radius, sweep):
    """How many samples an arc of `sweep` radians at `radius` deserves."""
    if radius <= 0.0:
        return 2
    # A chord subtending `theta` bows away from its arc by about r*theta^2/8.
    theta = math.sqrt(8.0 * CURVE_TOLERANCE / radius)
    return max(2, int(abs(sweep) / theta) + 2)
# Branch weight scales with the plot, so a small one does not turn into a
# grey disc.
BRANCH_WIDTH_AT = 420.0
BRANCH_WIDTH = 1.1

CLADE_FONT = 11.0
CLADE_LABEL_GAP = 7.0
# All the margin the plot keeps for itself. Everything else it is given, it
# uses.
PADDING = 8.0
# Below this radius the clade names are dropped rather than reserved for. The
# bands still read without them, and reserving room for text that will not fit
# is what stops a plot working when it shrinks.
LABEL_MIN_RADIUS = 150.0

INK = (58, 62, 70)
BRANCH_INK = (112, 120, 132)
TRACK_BG = (240, 242, 246)


def layout(root):
    """Give every node an angle and a distance from the root.

    Leaves are spread evenly around the circle — evenly by *count*, not by
    branch length, which is what keeps the bands aligned to the tips. An
    internal node sits at the midpoint of its children, so a branch always
    leaves from between what it joins.
    """
    tips = root.leaves()
    if not tips:
        return 0.0
    step = (2.0 * math.pi - OPEN_ANGLE) / max(len(tips) - 1, 1)
    for (i, leaf) in enumerate(tips):
        leaf.angle = START_ANGLE + step * i

    def resolve(node, depth):
        node.depth = depth + node.length
        for child in node.children:
            resolve(child, node.depth)
        if node.children:
            node.angle = sum(c.angle for c in node.children) / len(node.children)

    resolve(root, -root.length)
    return max(n.depth for n in root.walk())


def clade_of(root, names):
    """`{id(node): clade}` — the nearest named ancestor, itself included.

    Keyed by identity because the tree is parsed fresh each draw, so these
    objects never outlive the frame that made them.
    """
    out = {}

    def walk(node, current):
        here = node.name if node.name in names else current
        out[id(node)] = here
        for child in node.children:
            walk(child, here)

    walk(root, None)
    return out


def polar(cx, cy, radius, angle):
    return (cx + radius * math.cos(angle), cy + radius * math.sin(angle))




def arc(cx, cy, radius, a0, a1):
    """Points along the arc from `a0` to `a1`, inclusive of both ends.

    Sampled by *length* rather than by angle, which is what keeps the outer
    bands as smooth as the inner ones.
    """
    steps = arc_steps(radius, a1 - a0)
    return [
        polar(cx, cy, radius, a0 + (a1 - a0) * i / (steps - 1)) for i in range(steps)
    ]


def sector(cx, cy, r_in, r_out, a0, a1):
    """An annulus sector: out along one arc, back along the other."""
    return arc(cx, cy, r_out, a0, a1) + arc(cx, cy, r_in, a1, a0)


def sectors(cx, cy, r_in, r_out, a0, a1):
    """A filled band, as however many polygons it takes to fill correctly.

    A filled outline is triangulated by ear clipping, and that bridges across
    the hole when a band is thin and its sweep is wide: the straight chord
    between the ends of the outer arc dips below the inner arc, so a triangle
    spanning them covers the middle of the ring. That is the white wedge.

    Cutting the sweep until the chord stays inside the band is what prevents
    it. The sagitta of an arc of `sweep` at `r_out` is `r_out * (1 - cos(sweep
    / 2))`, and holding that to a fraction of the band's thickness leaves room
    to spare.
    """
    thickness = r_out - r_in
    if thickness <= 0.0 or r_out <= 0.0:
        return []
    limit = 1.0 - max(min(0.25 * thickness / r_out, 1.0), 0.0)
    widest = 2.0 * math.acos(max(-1.0, min(limit, 1.0)))
    sweep = a1 - a0
    parts = max(1, int(math.ceil(abs(sweep) / widest)))
    step = sweep / parts
    return [
        sector(cx, cy, r_in, r_out, a0 + step * i, a0 + step * (i + 1))
        for i in range(parts)
    ]


def branch_strokes(radius, root, at):
    """Every branch of `root` as a polyline, measured from the plot's centre.

    Each internal node contributes an arc spanning its children plus one
    radial line out to each of them. The arc is what carries the topology:
    two tips joined by a short one are close relatives, and one spanning half
    the circle is a deep split. Without it the tree is a starburst.

    The order is fixed by the walk, which is what lets a branch node built
    once keep answering for the same branch on every later frame.
    """
    out = []
    for node in root.walk():
        if node.is_leaf():
            continue
        r = at(node)
        angles = [child.angle for child in node.children]
        out.append(arc(0.0, 0.0, r, min(angles), max(angles)))
        for child in node.children:
            out.append(
                [
                    polar(0.0, 0.0, r, child.angle),
                    polar(0.0, 0.0, at(child), child.angle),
                ]
            )
    return out


def branch_inks(root, names):
    """The colour each branch starts in, in `branch_strokes` order.

    A branch takes the colour of the clade it sits inside, so a subtree reads
    as one. The backbone above every clade belongs to none of them and stays
    plain. The stroke out to a child is the child's: it is the branch that
    joins it, not the fork it left.
    """
    within = clade_of(root, names)
    out = []
    for node in root.walk():
        if node.is_leaf():
            continue
        out.append(names.get(within.get(id(node)), BRANCH_INK))
        for child in node.children:
            out.append(names.get(within.get(id(child)), BRANCH_INK))
    return out


def lerp_rgb(a, b, t):
    """The colour `t` of the way from `a` to `b`."""
    t = max(0.0, min(t, 1.0))
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


# ======================================================================
# The node
# ======================================================================


class Phylogeny:
    """A circular phylogeny with its annotation bands and clades.

    The Newick string is what is stored, not the parsed tree: a string is
    picklable and a tree of objects would need a `__reduce__`.
    """

    def __init__(self, newick=None, clades=None, branches=None):
        self.newick = NEWICK if newick is None else newick
        self.clades = dict(CLADES if clades is None else clades)
        # One `dex.Path` per branch stroke, built by `build` and owned by this
        # node. Each branch is a real node in the workspace, so it can be drawn
        # inspectably and recoloured on its own — the path *is* the state, and
        # nothing here has to remember what colour a branch was given.
        #
        # Their anchors are kept relative to the plot's centre, so the geometry
        # depends on the radius alone and survives the plot being moved.
        self.branches = list(branches or [])
        # The radius `self.branches` currently hold geometry for, and the one
        # seen last frame. Resyncing costs an action per branch, so it waits
        # for the size to settle: during a drag the radius differs from both
        # and the branches are drawn as plain values instead.
        self.geometry_radius = None
        self.seen_radius = None
        self.habitat = HABITAT
        self.resistant = list(RESISTANT)
        self.genome = list(GENOME_MB)
        self.gc = list(GC_PERCENT)

    # -- drawing ---------------------------------------------------------

    def draw(self, ctx):
        base = ctx.constraints
        width = base.x.provided_value() if base.x is not None else None
        height = base.y.provided_value() if base.y is not None else None
        if width is None or height is None:
            # A circle has no natural size; without a box there is nothing to fill.
            return dex.DrawResult.Complete(region=None)

        root = parse_newick(self.newick)
        span = layout(root)
        tips = root.leaves()
        if span <= 0.0 or not tips:
            return dex.DrawResult.Complete(region=None)

        half = min(width, height) / 2.0
        # Room for the clade names only when there is room to seat them. Below
        # that the plot still draws, one band smaller — which is what lets it
        # shrink to a canvas item instead of collapsing.
        named = half >= LABEL_MIN_RADIUS
        radius = self._fit(ctx, root, width, height) if named else half - PADDING
        if radius <= 0.0:
            return dex.DrawResult.Complete(region=None)
        cx = base.pos.x + width / 2.0
        cy = base.pos.y + height / 2.0
        step = (2.0 * math.pi - OPEN_ANGLE) / max(len(tips) - 1, 1)

        def at(node):
            """The radius a node sits at, by its distance from the root."""
            t = node.depth / span if span > 0 else 0.0
            return radius * (R_ROOT + (R_LEAF - R_ROOT) * t)

        self._branches(ctx, cx, cy, radius, root, at)
        self._habitat(ctx, cx, cy, radius, tips, step)
        self._resistance(ctx, cx, cy, radius, tips, step)
        self._genome(ctx, cx, cy, radius, tips, step)
        self._gc(ctx, cx, cy, radius, tips, step)
        self._clades(ctx, cx, cy, radius, root, step, named)

        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(
                base.pos, dex.Vector.new(width, height)
            )
        )

    def _origin(self, x=0.0, y=0.0):
        """Constraints placing a path's own coordinates at `(x, y)`.

        Left at the default it draws points that are already absolute; given
        the plot's centre it draws ones measured from it, which is how the
        branch nodes hold their geometry.
        """
        return dex.DrawConstraints(
            pos=dex.ScreenPos.new(x, y),
            x=None,
            y=None,
            wrap=None,  # NotAllowed
            should_clip=False,
        )

    def _stroke(self, ctx, points, rgb, width, origin=None):
        """An open polyline: a branch is a line, not an area."""
        if len(points) < 2:
            return
        ctx.draw_node(
            self._polyline(points, rgb, width),
            self._origin() if origin is None else origin,
        )

    def _polyline(self, points, rgb, width):
        return dex.Path.polyline(
            [dex.Vector.new(x, y) for (x, y) in points],
            dex.Stroke.new(width, dex.Color.rgb(*rgb)),
        )

    def _polygon(self, ctx, points, rgb, alpha=255):
        if len(points) < 3:
            return
        ctx.draw_node(
            dex.Path.polygon(
                [dex.Vector.new(x, y) for (x, y) in points],
                dex.Color.rgba(rgb[0], rgb[1], rgb[2], alpha),
                dex.Stroke.none(),
            ),
            self._origin(),
        )

    def _band(self, ctx, cx, cy, radius, r_in, r_out, rgb):
        """The well a per-tip band sits in.

        Stops at the open wedge rather than closing the circle, because that is
        what the tips do — and split by `sectors`, so its fill cannot bridge
        across the middle.
        """
        for part in sectors(
            cx,
            cy,
            radius * r_in,
            radius * r_out,
            START_ANGLE - OPEN_ANGLE / 2.0,
            START_ANGLE + 2.0 * math.pi - 1.5 * OPEN_ANGLE,
        ):
            self._polygon(ctx, part, rgb)

    def _cell(self, ctx, cx, cy, radius, r_in, r_out, angle, step, rgb, alpha=255):
        """One tip's block in a band, centred on its angle."""
        half = step * (0.5 - BLOCK_INSET)
        self._polygon(
            ctx,
            sector(cx, cy, radius * r_in, radius * r_out, angle - half, angle + half),
            rgb,
            alpha,
        )

    def _branches(self, ctx, cx, cy, radius, root, at):
        """Every branch, each its own inspectable node.

        A branch is a node in the workspace rather than a shape this plot
        paints, so clicking one opens its own inspector and the colour chosen
        there stays on it. That is the whole reason they are separate paths:
        the tree is not one drawing, it is a few hundred of them.

        The cost is that the geometry lives in those nodes, so it has to be
        pushed back out whenever the plot resizes — an action per branch. That
        waits for the size to settle (see `_resync`); until it has, the
        branches are drawn as plain values at the right size instead, so a
        drag stays smooth and nothing lags behind the bands.
        """
        weight = max(0.6, BRANCH_WIDTH * radius / BRANCH_WIDTH_AT)
        strokes = branch_strokes(radius, root, at)
        synced = self._resync(ctx, radius, strokes, weight)
        origin = self._origin(cx, cy)
        if synced:
            for uid in self.branches:
                ctx.draw_inspectable_node(uid, origin)
            return
        # Stale geometry: paint this frame ourselves rather than show the last
        # size. Not inspectable for the frames it takes the resize to settle.
        inks = branch_inks(root, self.clades)
        for (points, ink) in zip(strokes, inks):
            self._stroke(ctx, points, ink, weight, origin)

    def _resync(self, ctx, radius, strokes, weight):
        """Push `strokes` out to the branch nodes; are they showing them yet?

        Only once the radius has held still for a frame, so dragging a resize
        does not queue a few hundred actions per frame. One batch, so the whole
        tree moves in a single undo step.
        """
        if len(self.branches) != len(strokes):
            # A tree the branches were not built for — a newick swapped in
            # under a plain `Phylogeny()`. Nothing to sync, and nothing lost:
            # the caller paints it instead.
            return False
        if radius == self.geometry_radius:
            return True
        settled = radius == self.seen_radius
        self.seen_radius = radius
        if not settled:
            return False
        ws = ctx.node.workspace.action_handle()
        actions = []
        for (uid, points) in zip(self.branches, strokes):
            anchors = [dex.Anchor.corner(dex.Vector.new(x, y)) for (x, y) in points]
            actions.append((uid, dex.SetAnchors(anchors=anchors)))
            actions.append((uid, dex.SetPathStrokeWidth(width=weight)))
        ws.batch(actions, "Resized the phylogeny")
        self.geometry_radius = radius
        # The actions land before the next frame, not this one.
        return False

    def _habitat(self, ctx, cx, cy, radius, tips, step):
        """Where each isolate came from, as a categorical block."""
        self._band(ctx, cx, cy, radius, R_HABITAT_IN, R_HABITAT_OUT, TRACK_BG)
        for (i, leaf) in enumerate(tips):
            if i >= len(self.habitat):
                break
            entry = HABITATS.get(self.habitat[i])
            if entry is None:
                continue
            self._cell(
                ctx, cx, cy, radius, R_HABITAT_IN, R_HABITAT_OUT,
                leaf.angle, step, entry[1],
            )

    def _resistance(self, ctx, cx, cy, radius, tips, step):
        """Drawn only where present: absence is the empty well, not a colour."""
        self._band(ctx, cx, cy, radius, R_RESIST_IN, R_RESIST_OUT, TRACK_BG)
        for (i, leaf) in enumerate(tips):
            if i < len(self.resistant) and self.resistant[i]:
                self._cell(
                    ctx, cx, cy, radius, R_RESIST_IN, R_RESIST_OUT,
                    leaf.angle, step, RESIST_INK,
                )

    def _genome(self, ctx, cx, cy, radius, tips, step):
        """Assembly size, as a bar growing outward from the band's floor."""
        self._band(ctx, cx, cy, radius, R_GENOME_IN, R_GENOME_OUT, TRACK_BG)
        for (i, leaf) in enumerate(tips):
            if i >= len(self.genome):
                break
            t = max(0.0, min(self.genome[i] / GENOME_MAX, 1.0))
            top = R_GENOME_IN + (R_GENOME_OUT - R_GENOME_IN) * t
            self._cell(
                ctx, cx, cy, radius, R_GENOME_IN, top, leaf.angle, step, GENOME_INK
            )

    def _gc(self, ctx, cx, cy, radius, tips, step):
        """GC content, as a heat block: the ramp is the value."""
        self._band(ctx, cx, cy, radius, R_GC_IN, R_GC_OUT, TRACK_BG)
        (lo, hi) = GC_RANGE
        for (i, leaf) in enumerate(tips):
            if i >= len(self.gc):
                break
            t = (self.gc[i] - lo) / (hi - lo) if hi > lo else 0.0
            self._cell(
                ctx, cx, cy, radius, R_GC_IN, R_GC_OUT, leaf.angle, step,
                lerp_rgb(GC_LOW, GC_HIGH, t),
            )

    def _clades(self, ctx, cx, cy, radius, root, step, named):
        """A ring segment per named clade, spanning the leaves beneath it."""
        font = dex.Font.proportional(CLADE_FONT)
        wrap = dex.TextWrap.singleline()
        r_in = radius * R_CLADE_IN
        r_out = radius * R_CLADE_OUT
        for node in root.walk():
            if node.name not in self.clades:
                continue
            rgb = self.clades[node.name]
            angles = [leaf.angle for leaf in node.leaves()]
            if not angles:
                continue
            # Padded by half a tip, so the band brackets its leaves rather
            # than stopping on the centres of the outermost two.
            pad = step * 0.5
            (a0, a1) = (min(angles) - pad, max(angles) + pad)
            for part in sectors(cx, cy, r_in, r_out, a0, a1):
                self._polygon(ctx, part, rgb)
            if not named:
                continue

            middle = (a0 + a1) / 2.0
            metrics = ctx.measure_text(node.name, font, wrap)
            (x, y) = polar(cx, cy, r_out + CLADE_LABEL_GAP, middle)
            # Pushed out along its own radius, so it clears the ring on
            # whichever side of the circle it is on.
            x += math.cos(middle) * metrics.width / 2.0
            y += math.sin(middle) * metrics.height / 2.0
            label = dex.Label.new(node.name)
            label.font = font
            label.color = dex.Color.rgb(*INK)
            ctx.draw_node(
                label,
                dex.DrawConstraints(
                    pos=dex.ScreenPos.new(
                        x - metrics.width / 2.0, y - metrics.height / 2.0
                    ),
                    x=None,
                    y=None,
                    wrap=None,  # NotAllowed
                    should_clip=False,
                ),
            )

    def _fit(self, ctx, root, width, height):
        """The largest radius whose clade names still fit inside the box.

        A name sits outside the ring along its own radius, so it eats into the
        box sideways only as far as it points that way: one at the top costs
        height and almost no width. Reserving the longest name on every side —
        which is what a single margin does — throws most of the box away, and
        is why the plot looked so small in the middle of the space it had.

        Solved rather than guessed. A name of width `w` at angle `t` places its
        far edge at `(R + gap + w/2)|cos t| + w/2` from the centre, so the `R`
        that just fits the width is what that expression bounded by the
        half-width gives; the height falls out the same way from `|sin t|`.
        """
        half_w = width / 2.0 - PADDING
        half_h = height / 2.0 - PADDING
        # The circle itself has to fit before any label does.
        limit = min(half_w, half_h)

        font = dex.Font.proportional(CLADE_FONT)
        wrap = dex.TextWrap.singleline()
        for node in root.walk():
            if node.name not in self.clades:
                continue
            angles = [leaf.angle for leaf in node.leaves()]
            if not angles:
                continue
            middle = (min(angles) + max(angles)) / 2.0
            m = ctx.measure_text(node.name, font, wrap)
            (across, down) = (abs(math.cos(middle)), abs(math.sin(middle)))
            # A label pointing straight up puts no demand on the width at all,
            # so it is left out of that bound rather than dividing by nothing.
            if across > 1e-3:
                limit = min(
                    limit,
                    (half_w - m.width / 2.0) / across - CLADE_LABEL_GAP - m.width / 2.0,
                )
            if down > 1e-3:
                limit = min(
                    limit,
                    (half_h - m.height / 2.0) / down
                    - CLADE_LABEL_GAP
                    - m.height / 2.0,
                )
        return limit

    # -- messages --------------------------------------------------------

    def type_name(self):
        return "A Circular Phylogeny"

    def owned_nodes(self):
        """The branches, so a deep clone gets its own and a delete takes them."""
        return list(self.branches)

    def on_delete(self, ctx):
        for branch in self.branches:
            ctx.workspace.delete_node(branch)

    def build_inspector(self, ctx):
        """Nothing of its own.

        The plot as a whole has no settings worth a menu — what is worth
        inspecting is a branch, and each of those carries its own inspector.
        """
        return None


def build(ws, newick=None, clades=None):
    """A phylogeny with one path node built for each branch of its tree.

    The branches need a workspace to live in, which a bare constructor has no
    access to — so this is the way in. `Phylogeny()` on its own still draws,
    it just paints its branches instead of owning them, and they cannot be
    inspected one at a time.

    They are created empty: the geometry depends on the radius, which depends
    on the box, which nothing knows yet. The first draw fills them in.
    """
    palette = dict(CLADES if clades is None else clades)
    root = parse_newick(newick or NEWICK)
    layout(root)
    branches = [
        ws.insert_node_dyn(dex.Path.polyline([], dex.Stroke.new(BRANCH_WIDTH, dex.Color.rgb(*ink))))
        for ink in branch_inks(root, palette)
    ]
    return Phylogeny(newick=newick, clades=palette, branches=branches)


def transform():
    """A lambda returning the circular phylogeny of the hardcoded tree."""
    return build(dex.ws)
