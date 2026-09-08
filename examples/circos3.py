"""A circular phylogeny with annotation bands, iTOL style.

The third circos idiom: the ring is not an axis but the *leaves of a tree*, and
everything inside it is the topology that put them in that order. Rings of
per-tip annotation go outside, aligned to the leaves.

Two library pieces, composed:

  * `Phylogeny` in its `"circular"` shape draws the tree and records where every
    row landed. It is the same node `phylogeny.py` draws down the page, so a
    line drawn from here to a scatter lands on the right leaf.
  * `AnnotationRings` — the part this example owns — draws the bands outside,
    using the prelude's `sector_points` and the tree's own leaf order. It sits
    in the plane's foreground, so it stays crisp while the tree is magnified.

The bands are aligned by *asking the tree where it drew each row*, not by
recomputing the angles. That is the protocol doing real work inside one picture:
the ring and the tree agree because one of them is reading the other, so no
change to the tree's layout can leave the annotations pointing at the wrong tip.

With nothing wired it invents a small tree, so it opens onto a picture.
"""

import math

PLOT_SIZE = (1100.0, 1100.0)

#: How thick each annotation band is, and the gap before the first.
BAND_W = 22.0
BAND_GAP = 10.0


def sample():
    """Bacterial isolates in five clades, with three annotations apiece."""
    import random

    rng = random.Random(5)
    clades = {
        "Bacillota": ["Bacillus", "Staphylococcus", "Listeria", "Clostridium"],
        "Pseudomonadota": ["Escherichia", "Salmonella", "Pseudomonas", "Vibrio"],
        "Actinomycetota": ["Mycobacterium", "Streptomyces", "Corynebacterium"],
        "Bacteroidota": ["Bacteroides", "Prevotella"],
        "Campylobacterota": ["Campylobacter", "Helicobacter"],
    }
    rows = {"lineage": [], "clade": [], "genus": [],
            "host": [], "resistance": [], "genome_mb": []}
    hosts = ["human", "soil", "marine", "plant"]
    for (clade, genera) in clades.items():
        for genus in genera:
            for k in range(3):
                rows["lineage"].append(
                    "Bacteria;%s;%s;%s sp%d" % (clade, genus, genus, k + 1))
                rows["clade"].append(clade)
                rows["genus"].append(genus)
                rows["host"].append(rng.choice(hosts))
                rows["resistance"].append(rng.choice(["none", "single", "multi"]))
                rows["genome_mb"].append(round(rng.uniform(1.6, 7.4), 2))
    return rows


class AnnotationRings:
    """Bands of per-tip annotation outside a circular tree.

    Each band is one column. A categorical column gets a colour per level; a
    continuous one gets a bar out from the band's inner edge. Both are placed by
    asking the tree where it drew the row, so the rings cannot drift out of step
    with the topology inside them.
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
        (x, y) = (base.pos.x, base.pos.y)
        ws = ctx.node.workspace

        drawn = ws.send_request(self.tree, DrawnPoints()) or {}
        table = ws.send_request(self.tree, SourceTable())
        if not drawn or table is None:
            return dex.DrawResult.Complete(region=None)

        # The tree's centre and its outermost tip, read off what it drew — so
        # the rings sit outside whatever it actually did, at whatever zoom.
        points = [to_local(ctx, p) for p in drawn.values()]
        cx = sum(p[0] for p in points) / len(points)
        cy = sum(p[1] for p in points) / len(points)
        reach = max(math.hypot(p[0] - cx, p[1] - cy) for p in points)
        if reach <= 1.0:
            return dex.DrawResult.Complete(region=None)

        # One row's share of the ring, so a band is a wedge and not a hairline.
        width = 2.0 * math.pi / max(len(drawn), 1)
        for (band, column) in enumerate(self.columns):
            r0 = reach + BAND_GAP + band * BAND_W
            self.paint_band(ctx, table, drawn, column, cx, cy, r0, width, ctx)
            (cw, _ch) = measure(ctx, column, TICK_FONT)
            text(ctx, column, cx - cw / 2.0, cy - r0 - BAND_W, TICK_FONT, FAINT)

        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))

    def paint_band(self, ctx, table, drawn, column, cx, cy, r0, width, _ctx):
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

        for (row, point) in drawn.items():
            if row >= len(values):
                continue
            (px, py) = to_local(ctx, point)
            angle = math.atan2(py - cy, px - cx)
            value = values[row]
            if numeric:
                if not is_num(value):
                    continue
                height = (BAND_W - 3.0) * (float(value) - lo) / span
                (ink, r1) = (POINT, r0 + height)
            else:
                if value not in levels:
                    continue
                (ink, r1) = (palette[levels.index(value)], r0 + BAND_W - 3.0)
            polygon(ctx, sector_points(cx, cy, r0, r1,
                                       angle - width / 2.0, angle + width / 2.0), ink)


def transform():
    """The circular tree of `thisData`, ringed with its annotations."""
    ws = dex.ws
    frame = Frame(globals().get("thisData") or sample())

    lineage = next((c for c in frame.columns
                    if any(";" in str(v) for v in frame.values(c)[:32] if v is not None)),
                   None)
    if lineage is None:
        raise ValueError("wire a table with a lineage column into this transform")

    tree = build_plot(ws, Phylogeny, frame=frame, x=lineage,
                      color=globals().get("clade_column") or "clade",
                      shape="circular")
    tree.chrome = False
    body = ws.insert_node_dyn(tree)

    bands = [c for c in frame.columns if c != lineage][:4]
    rings = ws.insert_node_dyn(AnnotationRings(body, bands))
    readout = ws.insert_node_dyn(PlotChrome(body))
    return on_plane(ws, body, PLOT_SIZE, "Phylogeny", [rings, readout])
