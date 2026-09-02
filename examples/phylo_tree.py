"""A taxonomic tree, drawn on a canvas from a table of lineage strings.

Give it a `Table` with a column of the format every metagenomics tool speaks —

    d__Bacteria;p__Bacillota;c__Bacilli;...;s__Streptococcus pneumoniae

— and it builds a canvas holding the tree those lineages share. Wire the table
into a lambda argument named `table`; with nothing wired it draws the sample in
`lineage_table.py`, so it does something on its own.

The layout is a dendrogram lying on its side: depth runs left to right, one
column per rank, and taxa stack down the page with each parent centred on its
children. The canvas is the plane it sits on, so it pans and there is no edge
to run out of.

  * `RankAxis` bands a column per rank and heads it. The bands scroll sideways
    with the plane; the headings stay pinned to the top of the viewport, so you
    can always tell which column you are looking at.
  * `TreeLines` draws the brackets joining parent to child — a stub out of the
    parent, one vertical spanning every child, a stub into each — and every
    taxon's name. It is a background, so the dots sit over it rather than
    under.
  * The taxa are `StaticCanvasItem` circles, sized by read count where the
    table has one. They decline an inspector, so no lens appears over a dot and
    dragging across the tree pans it.
  * `TaxonReadout` names whatever the pointer is on, in full.

Nothing here needs a package. `lineage_table.py` needs `pyarrow` to *build* a
table; reading one only needs the column, which arrives as an ordinary list.

**On measuring text.** A name is as wide as it is, and the columns have to be
wide enough for the widest name in them — but a `Label` will not say how wide it
is until it has drawn. So `TreeLines` draws each name once, keeps the size it
reports, and asks for the frame to be thrown away (`request_skip_frame`). The
next frame has real widths and lays out properly, and the measuring pass is
never seen. The cache is dropped from `__getstate__`: it is worth keeping across
frames and worth nothing at all once saved.
"""

import math

# -- the plane -----------------------------------------------------------

# Vertical space per leaf, in canvas points.
ROW = 26.0
# Least horizontal space a rank column may take, before names widen it.
MIN_COLUMN = 90.0
# Room between the widest name in a column and the next column's dots.
COLUMN_PAD = 34.0
# Where the tree's own origin sits on the plane. Down and in, because canvas y
# grows downwards and a tree starting at zero would hang off the top edge.
TREE_X = 90.0
TREE_Y = 64.0

DOT_MIN = 2.5
DOT_MAX = 8.0
# How thick the branches are drawn.
BRANCH = 1.6
# Where a parent's vertical spine sits between its column and its children's.
SPINE_AT = 0.45
NAME_FONT = 12.0
# Room inside a name's box, and between the box and what it hangs off.
NAME_PAD = 4.0
NAME_GAP = 5.0
RANK_FONT = 17.0
# The band a heading sits in, at the top of the viewport.
HEADING_BAND = 30.0
READOUT_FONT = 11.0
RULER_INSET = 6.0
# How close the pointer must come to a dot, in canvas units, to name it.
HOVER_REACH = 13.0
# Roughly how wide a character is, for the one frame before anything has been
# measured and for sizing the readout box.
CHAR_W = 0.52

# Stored as plain RGB triples rather than `dex.Color` values: the palette is
# just data, and building the colour at draw time keeps these nodes picklable.
BAND = (236, 237, 239)
TREE = (26, 138, 92)
TEXT = (32, 38, 46)
HEADING = (24, 28, 34)
PANEL = (255, 255, 255)
PANEL_EDGE = (206, 211, 220)
NAME_FILL = (255, 255, 255)
NAME_EDGE = (214, 219, 226)

# Enough of a tree to draw with nothing wired in. `lineage_table.py` builds the
# same shape as a real table, with read counts; this is only so the example does
# something the moment it is opened.
SAMPLE_LINEAGES = [
    "d__Bacteria;p__Bacillota;c__Bacilli;o__Lactobacillales;"
    "f__Streptococcaceae;g__Streptococcus;s__Streptococcus pneumoniae",
    "d__Bacteria;p__Bacillota;c__Bacilli;o__Lactobacillales;"
    "f__Streptococcaceae;g__Streptococcus;s__Streptococcus pyogenes",
    "d__Bacteria;p__Bacillota;c__Bacilli;o__Bacillales;"
    "f__Staphylococcaceae;g__Staphylococcus;s__Staphylococcus aureus",
    "d__Bacteria;p__Bacillota;c__Clostridia;o__Eubacteriales;"
    "f__Clostridiaceae;g__Clostridium;s__Clostridium botulinum",
    "d__Bacteria;p__Pseudomonadota;c__Gammaproteobacteria;o__Enterobacterales;"
    "f__Enterobacteriaceae;g__Escherichia;s__Escherichia coli",
    "d__Bacteria;p__Pseudomonadota;c__Gammaproteobacteria;o__Enterobacterales;"
    "f__Enterobacteriaceae;g__Salmonella;s__Salmonella enterica",
    "d__Bacteria;p__Pseudomonadota;c__Gammaproteobacteria;o__Pseudomonadales;"
    "f__Pseudomonadaceae;g__Pseudomonas;s__Pseudomonas aeruginosa",
    "d__Bacteria;p__Actinomycetota;c__Actinomycetes;o__Mycobacteriales;"
    "f__Mycobacteriaceae;g__Mycobacterium;s__Mycobacterium tuberculosis",
    "d__Bacteria;p__Bacteroidota;c__Bacteroidia;o__Bacteroidales;"
    "f__Bacteroidaceae;g__Bacteroides;s__Bacteroides fragilis",
    "d__Archaea;p__Euryarchaeota;c__Methanobacteria;o__Methanobacteriales;"
    "f__Methanobacteriaceae;g__Methanobrevibacter;s__Methanobrevibacter smithii",
    "d__Archaea;p__Euryarchaeota;c__Halobacteria;o__Halobacteriales;"
    "f__Halobacteriaceae;g__Halobacterium;s__Halobacterium salinarum",
]

# The rank each depth stands for, by the prefix the lineage writes it with.
RANK_NAMES = {
    "d": "Domain",
    "k": "Kingdom",
    "p": "Phylum",
    "c": "Class",
    "o": "Order",
    "f": "Family",
    "g": "Genus",
    "s": "Species",
}


# ======================================================================
# Lineages — pure, and the part worth testing
# ======================================================================


def split_lineage(text):
    """A lineage string as a list of `(rank, name)`.

    Ranks are the one-letter prefixes the format uses; a field without one is
    given the empty rank rather than being dropped, because a name is a name
    whether or not the tool that wrote it said what rank it was.
    """
    out = []
    for field in text.split(";"):
        field = field.strip()
        if not field:
            continue
        rank, sep, name = field.partition("__")
        if sep and len(rank) <= 2:
            name = name.strip()
        else:
            rank, name = "", field
        # An unassigned rank is a real answer, not a name.
        if not name or name.lower() in ("unclassified", "unassigned", "na"):
            break
        out.append((rank, name))
    return out


def build_tree(lineages, weights=None):
    """The tree these lineages share.

    Returns `(nodes, roots)`, where a node is a dict carrying its name, rank,
    depth, parent, children and total weight. Nodes are keyed by their full
    path, so two genera of the same name under different families stay apart —
    homonyms across the tree of life are the rule, not the exception.
    """
    weights = weights or [1.0] * len(lineages)
    nodes = {}
    roots = []

    for lineage, weight in zip(lineages, weights):
        path = ()
        parent = None
        for depth, (rank, name) in enumerate(split_lineage(lineage)):
            path = path + (name,)
            node = nodes.get(path)
            if node is None:
                node = {
                    "key": path,
                    "name": name,
                    "rank": rank,
                    "depth": depth,
                    "parent": parent,
                    "children": [],
                    "weight": 0.0,
                }
                nodes[path] = node
                if parent is None:
                    roots.append(path)
                else:
                    nodes[parent]["children"].append(path)
            # Weight accumulates up the lineage, so an inner node carries what
            # everything beneath it carries.
            node["weight"] += weight
            parent = path
    return nodes, roots


def leaves_in_order(nodes, roots):
    """Every leaf, depth-first, which is the order they stack in."""
    out = []
    stack = list(reversed(roots))
    while stack:
        key = stack.pop()
        node = nodes[key]
        if node["children"]:
            stack.extend(reversed(node["children"]))
        else:
            out.append(key)
    return out


def assign_rows(nodes, roots):
    """A row for every node: leaves in order, parents centred on their children.

    Bottom-up, so a parent is only placed once its children have been.
    """
    rows = {}
    for row, key in enumerate(leaves_in_order(nodes, roots)):
        rows[key] = float(row)

    # Deepest first, so every node's children are already placed.
    for key in sorted(nodes, key=lambda k: nodes[k]["depth"], reverse=True):
        children = nodes[key]["children"]
        if children:
            rows[key] = sum(rows[c] for c in children) / len(children)
    return rows


def rank_labels(nodes):
    """The column heading for each depth, from the ranks the data carries."""
    by_depth = {}
    for node in nodes.values():
        by_depth.setdefault(node["depth"], node["rank"])
    depth = max(by_depth, default=-1)
    return [
        RANK_NAMES.get(by_depth.get(d, ""), f"Level {d + 1}") for d in range(depth + 1)
    ]


def column_x(widths):
    """The canvas x of each rank column, given the width each name needs.

    A column is as wide as its widest name, so a name never runs into the dots
    of the column after it.
    """
    out = []
    x = TREE_X
    for width in widths:
        out.append(x)
        x += max(MIN_COLUMN, width + COLUMN_PAD)
    return out


def dot_radius(weight, heaviest):
    """A dot sized by what it carries, on a square-root scale so area — not
    radius — tracks the count."""
    if heaviest <= 0.0:
        return DOT_MIN
    share = max(0.0, min(1.0, weight / heaviest))
    return DOT_MIN + (DOT_MAX - DOT_MIN) * math.sqrt(share)


# ======================================================================
# Reading the table
# ======================================================================


def lineage_column(source, column="lineage"):
    """The lineage strings in `source`, whatever `source` is.

    A wired `Table` arrives as a pyarrow table; a list of strings is a list of
    strings; and nothing at all falls back to the sample, so this draws
    something the moment it is opened.
    """
    if source is None:
        return list(SAMPLE_LINEAGES), None

    if isinstance(source, (list, tuple)):
        return [str(row) for row in source], None

    # A pyarrow table: take the named column, or the first one holding strings.
    names = list(getattr(source, "column_names", []))
    if column not in names:
        column = next((n for n in names if "linea" in n.lower() or "tax" in n.lower()), None)
        column = column or (names[0] if names else None)
    if column is None:
        return [], None

    lineages = [str(v) for v in source.column(column).to_pylist()]
    weights = None
    for candidate in ("reads", "count", "counts", "abundance"):
        if candidate in names:
            weights = [float(v or 0.0) for v in source.column(candidate).to_pylist()]
            break
    return lineages, weights


# ======================================================================
# Layers
# ======================================================================


class _Chrome:
    """What the layers below share: the mapping onto the plane."""

    def __init__(self, canvas):
        self.canvas = canvas

    def view(self, ctx):
        """`(origin, base, width, height)` for this frame, or `None` if the
        surface has gone."""
        origin = ctx.node.workspace.send_request(self.canvas, dex.CanvasViewOrigin())
        if origin is None:
            return None
        base = ctx.constraints
        return (
            origin,
            base,
            base.x.provided_value() if base.x is not None else 0.0,
            base.y.provided_value() if base.y is not None else 0.0,
        )

    def box(self, base, sx, sy, w=0.0, h=0.0):
        """Constraints for a box at `(sx, sy)` from the viewport's top-left.

        Screen offsets, not canvas ones: a caller that wants to scroll with the
        plane subtracts the view origin, and one that wants to stay put does
        not. That choice is the whole of the layer API.
        """
        return dex.DrawConstraints(
            pos=dex.ScreenPos.new(base.pos.x + sx, base.pos.y + sy),
            x=dex.AxisConstraint.Exactly(w) if w > 0.0 else None,
            y=dex.AxisConstraint.Exactly(h) if h > 0.0 else None,
            wrap=None,  # NotAllowed
            should_clip=False,
        )

    def line(self, ctx, base, sx, sy, dx, dy, rgb, thickness):
        ctx.draw_node(
            dex.Path.span(
                dex.Vector.new(dx, dy),
                dex.Stroke.new(thickness, dex.Color.rgb(*rgb)),
            ),
            self.box(base, sx, sy),
        )

    def label(self, value, rgb, size):
        node = dex.Label.new(value)
        node.font = dex.Font.proportional(size)
        node.color = dex.Color.rgb(*rgb)
        return node

    def text(self, ctx, base, sx, sy, value, rgb, size):
        ctx.draw_node(self.label(value, rgb, size), self.box(base, sx, sy))


class TreeLines(_Chrome):
    """The elbows joining parent to child, and every taxon's name.

    Also where the layout is worked out, because the layout is what the names
    are wide enough for. See the note in the module docstring for why that
    takes two frames the first time.
    """

    def __init__(self, canvas, nodes, roots):
        super().__init__(canvas)
        self.nodes = nodes
        self.roots = roots
        self.rows = assign_rows(nodes, roots)
        self.heaviest = max((n["weight"] for n in nodes.values()), default=0.0)
        # Measured `(width, height)` by name. Not state worth saving: see
        # `__getstate__`.
        self._sizes = {}

    def __getstate__(self):
        """Everything but the measurements.

        A cached size is worth keeping between frames and worth nothing after a
        save: it belongs to a font and a screen, not to the tree.
        """
        state = self.__dict__.copy()
        state["_sizes"] = {}
        return state

    # -- layout ----------------------------------------------------------

    def columns(self):
        """The canvas x of each rank column, from what has been measured."""
        depth = max((n["depth"] for n in self.nodes.values()), default=0)
        widths = []
        for d in range(depth + 1):
            names = [n["name"] for n in self.nodes.values() if n["depth"] == d]
            widths.append(max((self.width_of(n) for n in names), default=0.0))
        return column_x(widths)

    def size_of(self, name):
        """How big `name` draws: measured if it has been, estimated if not."""
        measured = self._sizes.get(name)
        if measured is not None:
            return measured
        return (len(name) * NAME_FONT * CHAR_W, NAME_FONT * 1.3)

    def width_of(self, name):
        return self.size_of(name)[0]

    def place(self, key, columns):
        """The canvas-space `(x, y)` of a node's dot."""
        node = self.nodes[key]
        return (columns[node["depth"]], TREE_Y + self.rows[key] * ROW)

    def measure(self, ctx, base):
        """Draw any unmeasured name once, to find out how wide it is.

        Returns whether anything was new — in which case the layout this frame
        was laid out against guesses, and the frame is not worth showing.
        """
        missing = [n["name"] for n in self.nodes.values() if n["name"] not in self._sizes]
        if not missing:
            return False
        for name in missing:
            drawn = ctx.draw_node(
                self.label(name, TEXT, NAME_FONT), self.box(base, 0.0, 0.0)
            )
            # The *field*, not the `region()` method on `DrawResult`: a
            # `Complete` carries one of each, and the field shadows the method.
            region = drawn.region
            size = region.size() if region is not None else None
            self._sizes[name] = (0.0, 0.0) if size is None else (size.x, size.y)
        return True

    # -- drawing ---------------------------------------------------------

    def draw(self, ctx):
        view = self.view(ctx)
        if view is None:
            return dex.DrawResult.Complete(region=None)
        (origin, base, width, height) = view

        if self.measure(ctx, base):
            # Laid out against guesses. Throw the frame away rather than show
            # it; the next one has real widths.
            ctx.request_skip_frame()
            return dex.DrawResult.Complete(region=None)

        columns = self.columns()
        for key, node in self.nodes.items():
            (cx, cy) = self.place(key, columns)
            radius = dot_radius(node["weight"], self.heaviest)
            (name_w, name_h) = self.size_of(node["name"])
            # Where the name sits, and where the branch out of it may start.
            name_x = cx + radius + NAME_GAP
            after_name = name_x + name_w + NAME_GAP

            # A bracket, the way a cladogram is drawn: a stub out of the
            # parent, one vertical spanning every child, and a stub into each.
            # Not an elbow per child — those would lay a dozen identical
            # verticals over each other and thicken the line by overdraw.
            #
            # The stub starts *after* the name rather than at the dot: a branch
            # running behind a label is a branch cut in half, boxed or not.
            children = node["children"]
            if children:
                rows = [self.place(child, columns) for child in children]
                spine = max(after_name, cx + (rows[0][0] - cx) * SPINE_AT)
                top = min(ky for (_kx, ky) in rows)
                bottom = max(ky for (_kx, ky) in rows)
                self.line(
                    ctx, base, after_name - origin.x, cy - origin.y,
                    spine - after_name, 0.0, TREE, BRANCH,
                )
                self.line(
                    ctx, base, spine - origin.x, top - origin.y, 0.0, bottom - top, TREE, BRANCH
                )
                for (kx, ky) in rows:
                    self.line(
                        ctx, base, spine - origin.x, ky - origin.y, kx - spine, 0.0, TREE, BRANCH
                    )

            # The name, in a box: over a grey band and among the branches, bare
            # text is hard to pick out, and the box is what separates it from
            # both. Sized from the measurement, so it fits the name exactly.
            box_y = cy - name_h * 0.5
            ctx.draw_node(
                dex.Rect.bordered(
                    name_w + 2.0 * NAME_PAD,
                    name_h + NAME_PAD,
                    dex.Color.rgb(*NAME_FILL),
                    3.0,
                    dex.Stroke.new(1.0, dex.Color.rgb(*NAME_EDGE)),
                ),
                self.box(
                    base,
                    name_x - NAME_PAD - origin.x,
                    box_y - NAME_PAD * 0.5 - origin.y,
                    name_w + 2.0 * NAME_PAD,
                    name_h + NAME_PAD,
                ),
            )
            self.text(
                ctx,
                base,
                name_x - origin.x,
                box_y - origin.y,
                node["name"],
                TEXT,
                NAME_FONT,
            )

        return dex.DrawResult.Complete(region=None)

    def type_name(self):
        return "Tree Lines"


class RankAxis(_Chrome):
    """A banded column per rank, headed at the top of the viewport.

    Alternating bands rather than rules: a name belongs to the column it sits
    in, and a band says which one that is without a line running through the
    tree. The bands scroll sideways with the plane and run the full height of
    the viewport, so panning down never leaves them behind; the headings do not
    scroll at all, so a column always says what it is.
    """

    def __init__(self, canvas, lines, headings):
        super().__init__(canvas)
        # Laid out by `TreeLines`; kept here so the axis need not repeat it.
        self.lines = lines
        self.headings = headings

    def band(self, index):
        """`(left, width)` of a rank's band, in canvas space."""
        left = self.lines[index] - COLUMN_PAD * 0.5
        if index + 1 < len(self.lines):
            return (left, self.lines[index + 1] - self.lines[index])
        return (left, MIN_COLUMN)

    def draw(self, ctx):
        view = self.view(ctx)
        if view is None:
            return dex.DrawResult.Complete(region=None)
        (origin, base, width, height) = view

        heading_font = dex.Font.proportional(RANK_FONT)
        heading_font.bold = True

        for index, heading in enumerate(self.headings):
            (left, band_w) = self.band(index)
            # Every other one, so neighbours are always told apart.
            if index % 2 == 0:
                ctx.draw_node(
                    dex.Rect.new(band_w, height, dex.Color.rgb(*BAND)),
                    self.box(base, left - origin.x, 0.0, band_w, height),
                )

            # Centred over the band, and pinned to the top of the viewport.
            # Estimated rather than measured: a heading is a caption, and a few
            # points off centre is invisible where a column's width is not.
            caption_w = len(heading) * RANK_FONT * CHAR_W
            label = dex.Label.new(heading)
            label.font = heading_font
            label.color = dex.Color.rgb(*HEADING)
            ctx.draw_node(
                label,
                self.box(
                    base,
                    left - origin.x + (band_w - caption_w) * 0.5,
                    RULER_INSET,
                ),
            )

        return dex.DrawResult.Complete(region=None)

    def type_name(self):
        return "Rank Axis"


class TaxonReadout(_Chrome):
    """The full lineage of whatever dot the pointer is on.

    One sensor for the whole tree, not one per dot: a hover-only
    `InteractionBox` across the viewport, asked where the pointer is, and the
    nearest taxon found by searching. A foreground, so it draws over the tree
    rather than under it.
    """

    def __init__(self, canvas, sensor, lines):
        super().__init__(canvas)
        self.sensor = sensor
        # `(canvas_x, canvas_y, name, lineage, weight)` per dot.
        self.spots = lines

    def owned_nodes(self):
        """The sensor is ours, and goes when we go."""
        return [self.sensor]

    def nearest(self, cx, cy):
        best = None
        for spot in self.spots:
            gap = (spot[0] - cx) ** 2 + (spot[1] - cy) ** 2
            if gap <= HOVER_REACH**2 and (best is None or gap < best[0]):
                best = (gap, spot)
        return None if best is None else best[1]

    def draw(self, ctx):
        view = self.view(ctx)
        if view is None:
            return dex.DrawResult.Complete(region=None)
        (origin, base, width, height) = view

        ctx.draw_node(
            self.sensor,
            dex.DrawConstraints(
                pos=base.pos,
                x=dex.AxisConstraint.Exactly(width),
                y=dex.AxisConstraint.Exactly(height),
                wrap=None,  # NotAllowed
                should_clip=False,
            ),
        )
        pointer = ctx.node.workspace.send_request(self.sensor, dex.PointerPos())
        if pointer is None:
            return dex.DrawResult.Complete(region=None)

        found = self.nearest(
            pointer.x - base.pos.x + origin.x, pointer.y - base.pos.y + origin.y
        )
        if found is None:
            return dex.DrawResult.Complete(region=None)
        (cx, cy, _name, lineage, weight) = found

        caption = lineage if weight is None else f"{lineage}    {int(weight):,} reads"
        caption_w = len(caption) * READOUT_FONT * CHAR_W + 14.0
        # Pinned to the bottom of the viewport: a lineage is far too long to sit
        # beside the dot without running off the edge.
        top = height - READOUT_FONT - 14.0
        left = min(max(RULER_INSET, cx - origin.x - caption_w / 2.0), max(RULER_INSET, width - caption_w - RULER_INSET))

        ctx.draw_node(
            dex.Circle.bordered(
                DOT_MAX + 4.0,
                dex.Color.transparent(),
                dex.Stroke.new(1.5, dex.Color.rgb(*TEXT)),
            ),
            self.box(base, cx - origin.x - DOT_MAX - 4.0, cy - origin.y - DOT_MAX - 4.0),
        )
        ctx.draw_node(
            dex.Rect.bordered(
                caption_w,
                READOUT_FONT + 10.0,
                dex.Color.rgba(PANEL[0], PANEL[1], PANEL[2], 240),
                4.0,
                dex.Stroke.new(1.0, dex.Color.rgb(*PANEL_EDGE)),
            ),
            self.box(base, left, top),
        )
        self.text(ctx, base, left + 7.0, top + 4.0, caption, TEXT, READOUT_FONT)

        return dex.DrawResult.Complete(region=None)

    def type_name(self):
        return "Taxon Readout"


# ======================================================================
# Building the tree
# ======================================================================


def build(ws, source=None):
    """A canvas holding the tree in `source`; returns its uid."""
    lineages, weights = lineage_column(source)
    nodes, roots = build_tree(lineages, weights)

    canvas = dex.Canvas.build(ws)
    lines = TreeLines(canvas, nodes, roots)

    # Where every dot sits, by the same layout the lines are drawn with. The
    # first frame's guessed widths only move the columns, and the items are
    # placed against the guess — so they are placed against it too, and agree.
    columns = lines.columns()
    heaviest = lines.heaviest
    spots = []
    placements = []
    for key, node in nodes.items():
        (cx, cy) = lines.place(key, columns)
        radius = dot_radius(node["weight"], heaviest)
        child = dex.NodeUid.mint()
        ws.insert_node_at_dyn(child, dex.Circle.new(radius, dex.Color.rgb(*TREE)))
        item = dex.StaticCanvasItem.build(
            ws,
            child,
            dex.Vector.new(cx - radius, cy - radius),
            dex.Vector.new(2.0 * radius, 2.0 * radius),
        )
        placements.append((canvas, dex.AdoptCanvasNode(item, dex.Layer.midground())))
        spots.append((cx, cy, node["name"], ";".join(key), node["weight"] if weights else None))

    sensor = ws.insert_node_dyn(dex.InteractionBox.sensing(True, False, False))
    chrome = [
        (RankAxis(canvas, columns, rank_labels(nodes)), dex.Layer.background()),
        (lines, dex.Layer.background()),
        (TaxonReadout(canvas, sensor, spots), dex.Layer.foreground()),
    ]
    for (node, layer) in chrome:
        uid = dex.NodeUid.mint()
        ws.insert_node_at_dyn(uid, node)
        ws.submit_action(canvas, dex.AdoptCanvasNode(uid, layer), "Added chrome")

    # One undo step for the whole tree, rather than one per taxon.
    ws.batch(placements, "Placed the taxa")
    return canvas


def transform():
    """A canvas of the tree in the wired table, or in the sample.

    A lambda's arguments are injected as globals, and only the wired ones — so
    a bare `table` would be a `NameError` until something is connected to it.
    Looked up instead, an unwired argument is simply `None`, which is what lets
    this draw its own sample and be worth opening on its own. Name it directly
    if the table is always going to be there.
    """
    return build(dex.ws, globals().get("table"))
