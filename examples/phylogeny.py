"""A vertical phylogenetic tree from a column of lineage strings.

Wire a `Table` into a lambda argument named `thisData` and this reads a column
whose values are semicolon-separated lineages —

    Eukaryota;Opisthokonta;Metazoa;Eumetazoa;Bilateria;Deuterostomia;Chordata;...

— and draws the tree they share, the way a Mermaid flowchart draws one: the root
at the top, each rank a row beneath the last, and square elbow connectors from a
parent down to a bus and out to each child. With nothing wired it draws a
built-in sample, so it opens onto something.

Unlike `phylo_tree.py`, the fields here carry no rank prefixes (`d__`, `p__`) —
they are just names — and the tree grows *down* the page rather than across it.
Otherwise the experience is `data_explorer.py`'s: hover a node for its lineage
and the rows behind it, click it for the whole record in an overlay (click empty
space to dismiss). There is no node per taxon — one sensor covers the tree and a
click searches for the nearest node, the way `pdb_viewer.py` picks an atom,
which is what keeps a big tree fast. And it answers the same query protocol from the
workspace prelude — `RowKeys`, `DrawnPoints`, `DrawnPoint`, `RowValues` — keying
each table row to the leaf its lineage ends on. So a tree and a `data_explorer`
built from one table can be wired together and, each frame, a third view can ask
both where row 7 is drawn and connect the two: the same record, in a tree and in
a scatter, joined by a line.

Layout is one node reflowed to its box, like the explorer. A large tree is drawn
small rather than scrolled; the sample is sized to be read whole.

Reading a lineage needs nothing installed — the column arrives as a list of
strings and `str.split` does the rest. Only a real wired `Table` arrives as a
pyarrow table, and its column is `.to_pylist()`.
"""

import math
import random

# ======================================================================
# The query protocol's tags (see prelude.py; kept identical)
# ======================================================================

ROW_KEYS = "dex.plot.row_keys"
DRAWN_POINTS = "dex.plot.drawn_points"
DRAWN_POINT = "dex.plot.drawn_point"
ROW_VALUES = "dex.plot.row_values"

# ======================================================================
# Look
# ======================================================================

INK = (32, 38, 46)
FAINT = (122, 128, 138)
BRANCH = (26, 138, 92)
PANEL = (255, 255, 255)
PANEL_EDGE = (208, 213, 222)
DOT = (26, 138, 92)
LEAF_INK = (24, 28, 34)

DOT_MIN = 3.0
DOT_MAX = 8.0
BRANCH_W = 1.6
NAME_FONT = 11.0
READOUT_FONT = 11.0
MARGIN = 18.0
TOP = 16.0            # room above the root
LEAF_BAND = 40.0      # room below the leaves for their names
HOVER_REACH = 13.0
CORNER = 5.0          # how far a mermaid elbow rounds its corner

# Enough of a tree to draw with nothing wired in — a few animals, a plant and a
# fungus, deep enough to branch.
SAMPLE_LINEAGES = [
    "Eukaryota;Opisthokonta;Metazoa;Bilateria;Deuterostomia;Chordata;Mammalia;Primates;Hominidae;Homo sapiens",
    "Eukaryota;Opisthokonta;Metazoa;Bilateria;Deuterostomia;Chordata;Mammalia;Primates;Hominidae;Pan troglodytes",
    "Eukaryota;Opisthokonta;Metazoa;Bilateria;Deuterostomia;Chordata;Mammalia;Carnivora;Felidae;Panthera leo",
    "Eukaryota;Opisthokonta;Metazoa;Bilateria;Deuterostomia;Chordata;Mammalia;Carnivora;Canidae;Canis lupus",
    "Eukaryota;Opisthokonta;Metazoa;Bilateria;Deuterostomia;Chordata;Aves;Passeriformes;Corvidae;Corvus corax",
    "Eukaryota;Opisthokonta;Metazoa;Bilateria;Protostomia;Arthropoda;Insecta;Diptera;Drosophilidae;Drosophila melanogaster",
    "Eukaryota;Opisthokonta;Metazoa;Bilateria;Protostomia;Arthropoda;Insecta;Hymenoptera;Apidae;Apis mellifera",
    "Eukaryota;Opisthokonta;Metazoa;Bilateria;Protostomia;Nematoda;Chromadorea;Rhabditida;Rhabditidae;Caenorhabditis elegans",
    "Eukaryota;Opisthokonta;Fungi;Dikarya;Ascomycota;Saccharomycetes;Saccharomycetales;Saccharomycetaceae;Saccharomyces cerevisiae",
    "Eukaryota;Archaeplastida;Viridiplantae;Streptophyta;Embryophyta;Magnoliopsida;Brassicales;Brassicaceae;Arabidopsis thaliana",
    "Eukaryota;Archaeplastida;Viridiplantae;Streptophyta;Embryophyta;Liliopsida;Poales;Poaceae;Oryza sativa",
]


# ======================================================================
# Lineages and the tree — pure, and the part worth testing
# ======================================================================


def split_lineage(text):
    """A lineage string as a list of names, in order from the root.

    Just the semicolon-separated fields, trimmed; an unassigned rank ends the
    lineage rather than becoming a name of its own.
    """
    out = []
    for field in str(text).split(";"):
        field = field.strip()
        if not field:
            continue
        if field.lower() in ("unclassified", "unassigned", "na", "environmental samples"):
            break
        out.append(field)
    return out


def build_tree(lineages):
    """The tree these `(row_id, lineage)` pairs share.

    Returns `(nodes, roots, row_leaf)`. A node is a dict of its name, depth,
    parent, children, the row ids whose lineage *ends* on it, and the total row
    count in its subtree (its weight). `row_leaf` maps a row id to the key of the
    node its lineage ends on — the leaf a table row is drawn at.
    """
    nodes = {}
    roots = []
    row_leaf = {}

    for row_id, lineage in lineages:
        names = split_lineage(lineage)
        if not names:
            continue
        path = ()
        parent = None
        for depth, name in enumerate(names):
            path = path + (name,)
            node = nodes.get(path)
            if node is None:
                node = {
                    "key": path, "name": name, "depth": depth, "parent": parent,
                    "children": [], "rows": [], "weight": 0,
                }
                nodes[path] = node
                if parent is None:
                    roots.append(path)
                else:
                    nodes[parent]["children"].append(path)
            node["weight"] += 1
            parent = path
        nodes[path]["rows"].append(row_id)
        row_leaf[row_id] = path
    return nodes, roots, row_leaf


def leaves_in_order(nodes, roots):
    """Every childless node, depth-first — the order they spread across the page."""
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


def assign_columns(nodes, roots):
    """A horizontal slot for every node: leaves spread in order, parents centred
    over their children."""
    cols = {}
    for i, key in enumerate(leaves_in_order(nodes, roots)):
        cols[key] = float(i)
    for key in sorted(nodes, key=lambda k: nodes[k]["depth"], reverse=True):
        children = nodes[key]["children"]
        if children:
            cols[key] = sum(cols[c] for c in children) / len(children)
    return cols


def dot_radius(weight, heaviest):
    """A dot sized by the rows under it, on a square-root scale so area tracks
    the count."""
    if heaviest <= 0:
        return DOT_MIN
    share = max(0.0, min(1.0, weight / heaviest))
    return DOT_MIN + (DOT_MAX - DOT_MIN) * math.sqrt(share)


# ======================================================================
# Reading the table
# ======================================================================


def read_lineages(source, column=None):
    """`(row_records, lineage_of, columns)` from whatever `source` is.

    `row_records` is one `{column: value}` dict per row, so a leaf can show the
    record behind it; `lineage_of` is the list of lineage strings, row-aligned.
    A list of strings is taken as the lineages themselves. Nothing at all falls
    back to the sample.
    """
    if source is None:
        lineages = list(SAMPLE_LINEAGES)
        records = [{"lineage": s} for s in lineages]
        return records, lineages, ["lineage"]

    if isinstance(source, (list, tuple)):
        lineages = [str(v) for v in source]
        records = [{"lineage": s} for s in lineages]
        return records, lineages, ["lineage"]

    if isinstance(source, dict):
        columns = list(source.keys())
        data = {c: list(source[c]) for c in columns}
    else:
        columns = list(source.column_names)
        data = {c: source.column(c).to_pylist() for c in columns}

    col = column if column in columns else _guess_lineage_column(columns, data)
    n = len(data[columns[0]]) if columns else 0
    records = [{c: data[c][i] for c in columns} for i in range(n)]
    lineages = [str(data[col][i]) if col else "" for i in range(n)]
    return records, lineages, columns


def _guess_lineage_column(columns, data):
    """The column most likely to hold lineages: one named for it, else the string
    column whose values carry semicolons."""
    for c in columns:
        if any(k in c.lower() for k in ("linea", "taxon", "tax", "clade", "path")):
            return c
    for c in columns:
        if any(isinstance(v, str) and ";" in v for v in data[c][:20]):
            return c
    return columns[0] if columns else None


def _show(v):
    if isinstance(v, float):
        return f"{v:.4g}"
    if v is None:
        return "—"
    return str(v)


def _inside(pointer, rect):
    (x, y, w, h) = rect
    return x <= pointer.x <= x + w and y <= pointer.y <= y + h


# ======================================================================
# The tree
# ======================================================================


class Phylogeny:
    """The whole tree as one node: the branches, the dots, the names, the hover
    readout, the click overlay — and the query protocol keyed by table row.

    No node per taxon: one sensor covers the tree, and hover and click are
    answered by searching the node positions drawn this frame — the way
    `pdb_viewer.py` picks an atom, and what keeps a big tree fast. It keeps every
    table row's record so it can answer `RowValues`, and records where it drew
    each row's leaf so it can answer `DrawnPoint`. `_node_pos` covers every node,
    for hover; `_frame_pos` is the leaves keyed by row id, which is what another
    view links against.
    """

    def __init__(self, nodes, roots, row_leaf, records, columns, sensor):
        self.nodes = nodes
        self.roots = roots
        self.row_leaf = row_leaf          # row_id -> node key
        self.records = records            # [{column: value}] per table row
        self.columns = list(columns)
        self.n = len(records)
        self.sensor = sensor
        # The rows ending on each node, so a click can show them.
        self.rows_of = {}
        for row_id, key in row_leaf.items():
            self.rows_of.setdefault(key, []).append(row_id)
        self.cols = assign_columns(nodes, roots)
        self.max_depth = max((nd["depth"] for nd in nodes.values()), default=0)
        self.leaf_count = max(1, len(leaves_in_order(nodes, roots)))
        self.heaviest = max((nd["weight"] for nd in nodes.values()), default=0)
        self._node_pos = {}               # node key -> (x, y)
        self._frame_pos = {}              # row_id -> (x, y)
        self.selected = None              # a node key a click chose, or None
        self._overlay_rect = None

    def __getstate__(self):
        state = self.__dict__.copy()
        state["_node_pos"] = {}
        state["_frame_pos"] = {}
        state["_overlay_rect"] = None
        return state

    def owned_nodes(self):
        return [self.sensor] if self.sensor else []

    def on_delete(self, ctx):
        for uid in self.owned_nodes():
            ctx.workspace.delete_node(uid)

    def type_name(self):
        return "A Phylogeny"

    # -- the query protocol ----------------------------------------------

    def request(self, req, ctx):
        tag = getattr(req, "name", None)
        if tag == ROW_KEYS:
            return list(range(self.n))
        if tag == DRAWN_POINTS:
            return dict(self._frame_pos)
        if tag == DRAWN_POINT:
            return self._frame_pos.get(req.row_id)
        if tag == ROW_VALUES:
            if 0 <= req.row_id < self.n:
                return dict(self.records[req.row_id])
            return None
        return NotImplemented

    # -- draw helpers ----------------------------------------------------

    def _abs(self):
        return dex.DrawConstraints(
            pos=dex.ScreenPos.new(0.0, 0.0), x=None, y=None, wrap=None, should_clip=False)

    def _at(self, x, y, w=None, h=None):
        return dex.DrawConstraints(
            pos=dex.ScreenPos.new(x, y),
            x=dex.AxisConstraint.Exactly(w) if w is not None else None,
            y=dex.AxisConstraint.Exactly(h) if h is not None else None,
            wrap=None, should_clip=False)

    def _line(self, ctx, pts, rgb, w=BRANCH_W):
        ctx.draw_node(
            dex.Path.polyline([dex.Vector.new(x, y) for (x, y) in pts],
                              dex.Stroke.new(w, dex.Color.rgb(*rgb))),
            self._abs())

    def _dot(self, ctx, cx, cy, r, rgb):
        ctx.draw_node(dex.Circle.new(r, dex.Color.rgb(*rgb)), self._at(cx - r, cy - r))

    def _text(self, ctx, s, x, y, size, rgb, bold=False):
        lab = dex.Label.new(s)
        font = dex.Font.proportional(size)
        font.bold = bold
        lab.font = font
        lab.color = dex.Color.rgb(*rgb)
        ctx.draw_node(lab, self._at(x, y))

    def _measure(self, ctx, s, size, bold=False):
        font = dex.Font.proportional(size)
        font.bold = bold
        m = ctx.measure_text(s, font, dex.TextWrap.singleline())
        return (m.width, m.height)

    # -- drawing ---------------------------------------------------------

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        h = base.y.provided_value() if base.y is not None else None
        if w is None or h is None or not (math.isfinite(w) and math.isfinite(h)):
            return dex.DrawResult.Complete(region=None)
        (ox, oy) = (base.pos.x, base.pos.y)
        self._node_pos = {}
        self._frame_pos = {}

        ctx.draw_node(
            dex.Rect.bordered(w, h, dex.Color.rgb(*PANEL), 0.0,
                              dex.Stroke.new(1.0, dex.Color.rgb(*PANEL_EDGE))),
            self._at(ox, oy, w, h))

        if not self.nodes:
            self._text(ctx, "no lineages to draw", ox + MARGIN, oy + MARGIN, NAME_FONT, FAINT)
            return self._done(base, w, h)

        # Map the tree's slots onto the box: columns across, depth down.
        left = ox + MARGIN
        right = ox + w - MARGIN
        top = oy + TOP + 8.0
        bottom = oy + h - LEAF_BAND
        span_x = right - left
        rows = max(1, self.max_depth)

        def px(col):
            if self.leaf_count <= 1:
                return (left + right) / 2.0
            return left + col / (self.leaf_count - 1) * span_x

        def py(depth):
            return top + depth / rows * (bottom - top)

        # Position every node first, so branches can be drawn to real points.
        for key, node in self.nodes.items():
            self._node_pos[key] = (px(self.cols[key]), py(node["depth"]))
        for row_id, key in self.row_leaf.items():
            self._frame_pos[row_id] = self._node_pos[key]

        # The branches: a Mermaid-style elbow from each parent to its children —
        # a stub down, a bus across, a stub down to each — drawn under the dots.
        for key, node in self.nodes.items():
            children = node["children"]
            if not children:
                continue
            (pxp, pyp) = self._node_pos[key]
            kids = [self._node_pos[c] for c in children]
            bus = (pyp + min(ky for _, ky in kids)) / 2.0
            self._line(ctx, [(pxp, pyp), (pxp, bus)], BRANCH)
            left_x = min(kx for kx, _ in kids)
            right_x = max(kx for kx, _ in kids)
            self._line(ctx, [(left_x, bus), (right_x, bus)], BRANCH)
            for (kx, ky) in kids:
                self._elbow(ctx, kx, bus, ky)

        # The dots and names, over the branches.
        for key, node in self.nodes.items():
            (cx, cy) = self._node_pos[key]
            radius = dot_radius(node["weight"], self.heaviest)
            self._dot(ctx, cx, cy, radius, DOT)
            if not node["children"]:
                self._leaf_name(ctx, node["name"], cx, cy + radius + 4.0)
            elif node["depth"] == 0 or len(node["children"]) > 1:
                # Name the forks that matter, above their dot.
                (nw, _) = self._measure(ctx, node["name"], NAME_FONT)
                self._text(ctx, node["name"], cx - nw / 2.0, cy - radius - 14.0,
                           NAME_FONT, FAINT)

        self._interact(ctx, ctx.node.workspace, ox, oy, w, h)
        return self._done(base, w, h)

    def _elbow(self, ctx, kx, bus, ky):
        """A vertical drop from the bus to a child, with a rounded corner where it
        leaves the bus — the small radius a Mermaid edge turns with."""
        self._line(ctx, [(kx, bus + CORNER), (kx, ky)], BRANCH)

    def _leaf_name(self, ctx, name, cx, top_y):
        """A leaf's name under its dot, truncated to the room a leaf has."""
        slot = 0.0
        if self.leaf_count > 1:
            # The horizontal room one leaf gets.
            slot = (ctx.constraints.x.provided_value() - 2.0 * MARGIN) / max(self.leaf_count - 1, 1)
        text = name
        limit = max(slot, 64.0)
        while text and self._measure(ctx, text, NAME_FONT)[0] > limit and len(text) > 1:
            text = text[:-1]
        (tw, _) = self._measure(ctx, text, NAME_FONT)
        self._text(ctx, text, cx - tw / 2.0, top_y, NAME_FONT, LEAF_INK)

    def _done(self, base, w, h):
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))

    # -- hover, click and the overlay ------------------------------------

    def _nearest(self, pointer):
        """The node key nearest the pointer, within reach, or None."""
        best = None
        for key, (cx, cy) in self._node_pos.items():
            gap = (cx - pointer.x) ** 2 + (cy - pointer.y) ** 2
            if gap <= HOVER_REACH ** 2 and (best is None or gap < best[0]):
                best = (gap, key)
        return None if best is None else best[1]

    def _interact(self, ctx, ws, ox, oy, w, h):
        """One sensor over the tree: hover names a node, a click selects it (or
        clears the overlay on empty space)."""
        if self.sensor is None:
            return
        ctx.draw_node(
            self.sensor,
            dex.DrawConstraints(
                pos=dex.ScreenPos.new(ox, oy),
                x=dex.AxisConstraint.Exactly(w),
                y=dex.AxisConstraint.Exactly(h),
                wrap=None, should_clip=False))
        pointer = ws.send_request(self.sensor, dex.PointerPos())
        hovered = self._nearest(pointer) if pointer is not None else None

        if ws.send_request(self.sensor, dex.TakeClicked()) and pointer is not None:
            if self._overlay_rect and _inside(pointer, self._overlay_rect):
                pass
            elif hovered is not None:
                self.selected = hovered
            else:
                self.selected = None

        if hovered is not None:
            self._hover_readout(ctx, ox, oy, w, h, hovered)
        self._overlay_rect = None
        if self.selected is not None and self.selected in self.nodes:
            self._detail_overlay(ctx, ox, oy, w, h, self.selected)

    def _hover_readout(self, ctx, ox, oy, w, h, key):
        """The full lineage of the hovered node, pinned along the bottom."""
        (cx, cy) = self._node_pos[key]
        node = self.nodes[key]
        lineage = " › ".join(key)
        caption = lineage if not node["rows"] else "%s   (%d row%s)" % (
            lineage, len(node["rows"]), "" if len(node["rows"]) == 1 else "s")
        (cw, _) = self._measure(ctx, caption, READOUT_FONT)
        cap_w = cw + 14.0
        top = oy + h - READOUT_FONT - 16.0
        left = min(max(ox + 6.0, cx - cap_w / 2.0), ox + w - cap_w - 6.0)
        ctx.draw_node(
            dex.Circle.bordered(DOT_MAX + 4.0, dex.Color.transparent(),
                                dex.Stroke.new(1.5, dex.Color.rgb(*INK))),
            self._at(cx - DOT_MAX - 4.0, cy - DOT_MAX - 4.0))
        ctx.draw_node(
            dex.Rect.bordered(cap_w, READOUT_FONT + 12.0, dex.Color.rgba(255, 255, 255, 244),
                              4.0, dex.Stroke.new(1.0, dex.Color.rgb(*PANEL_EDGE))),
            self._at(left, top, cap_w, READOUT_FONT + 12.0))
        self._text(ctx, caption, left + 7.0, top + 5.0, READOUT_FONT, INK)

    def _detail_overlay(self, ctx, ox, oy, w, h, key):
        """The clicked taxon in full — its lineage rank by rank, and the rows
        that end on it — pinned to the top-right. Click empty space to dismiss.
        """
        node = self.nodes[key]
        (cx, cy) = self._node_pos[key]
        ctx.draw_node(
            dex.Circle.bordered(DOT_MAX + 6.0, dex.Color.transparent(),
                                dex.Stroke.new(2.0, dex.Color.rgb(*DOT))),
            self._at(cx - DOT_MAX - 6.0, cy - DOT_MAX - 6.0))

        lines = [node["name"]]
        lines += ["  " + ("› " if d else "") + n for d, n in enumerate(key)]
        here = self.rows_of.get(key, [])
        if here:
            lines.append("")
            lines.append("%d row%s here:" % (len(here), "" if len(here) == 1 else "s"))
            for r in here[:6]:
                rec = self.records[r]
                lines.append("  · " + ", ".join("%s %s" % (c, _show(rec[c]))
                                                for c in self.columns))
            if len(here) > 6:
                lines.append("  … and %d more" % (len(here) - 6))
        elif node["children"]:
            lines.append("")
            lines.append("%d rows in this clade" % node["weight"])

        row_h = READOUT_FONT + 5.0
        cap_w = max(self._measure(ctx, s, READOUT_FONT)[0] for s in lines) + 18.0
        cap_h = len(lines) * row_h + 12.0
        left = ox + w - cap_w - 6.0
        top = oy + 6.0
        self._overlay_rect = (left, top, cap_w, cap_h)
        ctx.draw_node(
            dex.Rect.bordered(cap_w, cap_h, dex.Color.rgba(255, 255, 255, 250), 5.0,
                              dex.Stroke.new(1.0, dex.Color.rgb(*PANEL_EDGE))),
            self._at(left, top, cap_w, cap_h))
        for k, s in enumerate(lines):
            self._text(ctx, s, left + 9.0, top + 6.0 + k * row_h, READOUT_FONT, INK,
                       bold=(k == 0))


# ======================================================================
# Building
# ======================================================================


def build(ws, source=None, column=None):
    """A `Phylogeny` node over `source`; returns the node object."""
    records, lineages, columns = read_lineages(source, column)
    nodes, roots, row_leaf = build_tree(list(enumerate(lineages)))
    # One sensor over the whole tree: hover to read a node, click to select it.
    # It does not sense drags, so on a plane the surface still pans.
    sensor = ws.insert_node_dyn(dex.InteractionBox.sensing(True, True, False))
    return Phylogeny(nodes, roots, row_leaf, records, columns, sensor)


def transform():
    """A phylogeny over the wired `thisData`, or over the built-in sample."""
    return build(dex.ws, globals().get("thisData"), globals().get("column"))
