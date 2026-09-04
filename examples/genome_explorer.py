"""A whole-genome map from a GenBank flat file, BioCyc Genome-Explorer style.

Give the transform a `.gbff`/`.gb` file's text as `genebank_data` and it draws
the replicon the way BioCyc's genome overview does: the sequence wrapped into
horizontal rows, a coordinate ruler down the left, and every gene a strand-aware
arrow — forward above the line, reverse below — coloured by feature type.

The point of this example, and the reason it is built the way it is: **each gene
is its own node in the workspace, not paint this plot lays down.** Click one and
the inspector opens on the gene itself, showing its name, locus tag, product and
location — the BioCyc hover popup, but as a real, selectable object rather than a
tooltip. That is what `draw_inspectable_node` buys: the plot hands the workspace
a box per gene, the gene draws its arrow into that box, and the click lands on
the gene.

A gene needs nothing pushed into it to be drawn at the right place, unlike the
phylogeny's branches: an arrow is defined by its bounding box and its strand, so
the box the plot allots each frame *is* the geometry. No resync, no stored
anchors — the gene reads its size from the constraints and draws. That keeps a
few thousand gene-nodes cheap enough to redraw every frame without the caching
the tree needed.

Going with the format: the parser handles the common GenBank grammar — `LOCUS`
length, `FEATURES`, `key  location`, indented `/qualifier="value"` — across
however many records the file holds (a draft genome is many contigs), laid end
to end into one wrapped map. It does not read `join(...)` segment by segment; a
feature spanning them is drawn from its first base to its last, which is what an
overview wants.
"""

import math
import re

# ======================================================================
# What to draw, and in what colour
# ======================================================================

# The feature keys that get an arrow, and their colours. A key not listed here
# — `gene` (redundant with `CDS`), `source`, `repeat_region` — is skipped. Add a
# key to draw it; the legend follows this table.
FEATURE_COLORS = {
    "CDS": (86, 124, 176),
    "tRNA": (94, 168, 116),
    "rRNA": (206, 132, 84),
    "ncRNA": (150, 110, 180),
    "tmRNA": (176, 120, 168),
    "misc_RNA": (120, 140, 152),
    "regulatory": (200, 176, 90),
    "mobile_element": (150, 150, 156),
}

INK = (58, 62, 70)
FAINT = (120, 126, 136)
LINE_INK = (168, 174, 184)
CONTIG_INK = (150, 96, 96)

# ======================================================================
# GenBank parsing
# ======================================================================


def parse_location(loc):
    """`(start, end, strand)` for a GenBank location, or None.

    Strand is -1 under `complement`, else +1. `join`/`order` and the `<`/`>`
    partials collapse to the span from the first coordinate to the last — enough
    for an overview arrow.
    """
    strand = -1 if "complement" in loc else 1
    nums = re.findall(r"\d+", loc)
    if not nums:
        return None
    ints = [int(n) for n in nums]
    return (min(ints), max(ints), strand)


def parse_genbank(text):
    """Every record in `text`, as dicts of name, length, definition, features.

    A small reader for the flat-file grammar: `LOCUS` opens a record, `FEATURES`
    begins the table, a feature is a key at column 6 with its location, and its
    qualifiers are the `/name="value"` lines indented beneath it. `//` closes the
    record. It does not touch the sequence, and it drops `/translation` — a whole
    protein per CDS is a lot of text no map needs.
    """
    records = []
    cur = None
    lines = text.splitlines()
    i, n = 0, len(lines)

    while i < n:
        line = lines[i]
        if line.startswith("LOCUS"):
            cur = {"name": "", "length": 0, "definition": "", "features": []}
            parts = line.split()
            if len(parts) >= 2:
                cur["name"] = parts[1]
            for (j, p) in enumerate(parts):
                if p == "bp" and j > 0:
                    try:
                        cur["length"] = int(parts[j - 1].replace(",", ""))
                    except ValueError:
                        pass
            records.append(cur)
            i += 1
            continue
        if cur is not None and line.startswith("DEFINITION"):
            cur["definition"] = line[12:].strip()
            i += 1
            while i < n and lines[i].startswith(" " * 12):
                cur["definition"] += " " + lines[i].strip()
                i += 1
            continue
        if cur is not None and line.startswith("FEATURES"):
            i += 1
            i = _parse_features(lines, i, n, cur)
            continue
        if line.startswith("//"):
            cur = None
        i += 1

    return records


def _parse_features(lines, i, n, rec):
    """Read the FEATURES block into `rec`, returning the line after it."""
    while i < n:
        fl = lines[i]
        if fl.strip() and not fl[:1].isspace():
            break  # ORIGIN, //, CONTIG, another LOCUS — the table is over.
        if fl.strip() and not fl[:6].isspace():
            key = fl[5:21].strip()
            loc = fl[21:].strip()
            quals = {}
            i += 1
            last = None
            while i < n and lines[i].strip() and lines[i][:6].isspace():
                s = lines[i].strip()
                if s.startswith("/"):
                    if "=" in s:
                        (k, v) = s[1:].split("=", 1)
                        if k == "translation":
                            quals[k] = True
                        else:
                            quals[k] = v.strip().strip('"')
                        last = k
                    else:
                        quals[s[1:]] = True
                        last = s[1:]
                elif last == "translation":
                    pass  # skip the protein sequence's continuation lines
                elif last and isinstance(quals.get(last), str):
                    joiner = "" if quals[last].endswith("-") else " "
                    quals[last] = quals[last] + joiner + s.strip('"')
                else:
                    loc += s  # a location that wrapped onto the next line
                i += 1
            rec["features"].append((key, loc, quals))
        else:
            i += 1
    return i


# ======================================================================
# Geometry
# ======================================================================

MARGIN = 12.0
TITLE_H = 22.0
LEGEND_H = 24.0
RULER_GUTTER = 66.0     # left column for the bp coordinate of each row
ROW_GAP = 16.0
BAND = 8.0              # height of one strand's glyph band
LINE_HALF = 5.0         # gap between the centre line and a glyph band
GENE_MIN_PX = 1.2       # a gene is never thinner than this, so it stays visible
GENE_LABEL_MIN = 34.0   # a name is drawn inside a glyph only this wide or wider

TITLE_FONT = 13.0
COORD_FONT = 9.5
LEGEND_FONT = 10.0
GENE_FONT = 9.0

ROW_H = 2.0 * BAND + 2.0 * LINE_HALF + ROW_GAP


def arrow_points(x0, x1, y0, y1, strand):
    """A strand-aware gene glyph: a box with a pointed head, or a plain box.

    Points right on the forward strand, left on the reverse. Below a few points
    of width there is no room for a head, so it degrades to a rectangle.
    """
    head = min((x1 - x0) * 0.5, y1 - y0)
    if x1 - x0 < 3.0 or head < 1.5:
        return [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    ymid = (y0 + y1) / 2.0
    if strand >= 0:
        return [(x0, y0), (x1 - head, y0), (x1, ymid), (x1 - head, y1), (x0, y1)]
    return [(x1, y0), (x0 + head, y0), (x0, ymid), (x0 + head, y1), (x1, y1)]


def _darker(rgb, f=0.72):
    return tuple(int(round(c * f)) for c in rgb)


def format_bp(n):
    return "{:,}".format(n)


# ======================================================================
# One gene: a node with an arrow and an inspector
# ======================================================================


class Gene:
    """A single feature: the arrow the plot draws, and the data a click shows.

    Drawn into whatever box the plot allots it — the box is the geometry, so the
    same gene draws at any size with nothing stored. Its `build_inspector`
    returns its own record, which the workspace seats in the inspector when the
    gene is clicked.
    """

    def __init__(self, key, name, locus, product, start, end, strand, color, contig):
        self.key = key
        self.name = name
        self.locus = locus
        self.product = product
        self.start = start          # local to the contig, for the readout
        self.end = end
        self.strand = strand
        self.color = tuple(color)
        self.contig = contig

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else 0.0
        h = base.y.provided_value() if base.y is not None else 0.0
        if w <= 0.0 or h <= 0.0:
            return dex.DrawResult.Complete(region=None)
        (x0, y0) = (base.pos.x, base.pos.y)
        (x1, y1) = (x0 + w, y0 + h)
        pts = arrow_points(x0, x1, y0, y1, self.strand)
        ctx.draw_node(
            dex.Path.polygon(
                [dex.Vector.new(px, py) for (px, py) in pts],
                dex.Color.rgb(*self.color),
                dex.Stroke.new(0.75, dex.Color.rgb(*_darker(self.color))),
            ),
            self._abs(),
        )
        label = self.name or self.locus
        if w >= GENE_LABEL_MIN and label:
            font = dex.Font.proportional(GENE_FONT)
            wrap = dex.TextWrap.singleline()
            m = ctx.measure_text(label, font, wrap)
            if m.width <= w - 8.0 and m.height <= h:
                text = dex.Label.new(label)
                text.font = font
                text.color = dex.Color.rgb(255, 255, 255)
                ctx.draw_node(
                    text,
                    dex.DrawConstraints(
                        pos=dex.ScreenPos.new(
                            x0 + (w - m.width) / 2.0, y0 + (h - m.height) / 2.0
                        ),
                        x=None, y=None, wrap=None, should_clip=False,
                    ),
                )
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h))
        )

    def _abs(self):
        return dex.DrawConstraints(
            pos=dex.ScreenPos.new(0.0, 0.0),
            x=None, y=None, wrap=None, should_clip=False,
        )

    def type_name(self):
        who = self.name or self.locus or self.key
        return "%s  (%s)" % (who, self.key)

    def build_inspector(self, ctx):
        """The gene's record, as the panel a click opens.

        Returned as a value; the workspace gives it a home and seats it under the
        type name in the inspector. No node has to be built here by hand.
        """
        rows = []
        if self.name:
            rows.append("Gene: %s" % self.name)
        if self.locus:
            rows.append("Locus tag: %s" % self.locus)
        rows.append("Type: %s" % self.key)
        if self.product:
            rows.append("Product: %s" % self.product)
        if self.contig:
            rows.append("Contig: %s" % self.contig)
        arrow = "→" if self.strand >= 0 else "←"
        rows.append("Location: %s %s %s" % (format_bp(self.start), arrow, format_bp(self.end)))
        rows.append("Length: %s bp" % format_bp(self.end - self.start + 1))
        panel = dex.Label.new("\n".join(rows))
        # Without this the newlines render as tofu boxes on one truncated line —
        # a Label is singleline by default.
        panel.singleline = False
        panel.font = dex.Font.proportional(11.0)
        panel.color = dex.Color.rgb(*INK)
        return panel


# ======================================================================
# The genome: the ruler, the rows, and every gene laid onto them
# ======================================================================


class GenomeExplorer:
    """The wrapped genome map. Owns a `Gene` node per drawn feature.

    Stores only what the layout needs — each gene's id and its global span — and
    lets the genes carry their own data. The genes are built by `build`, which
    has a workspace to put them in; the bare constructor draws what it is given.
    """

    def __init__(self, records, offsets, genes, total_len, types_present):
        self.records = list(records)          # [(name, length, definition)]
        self.offsets = list(offsets)          # global start of each contig
        self.genes = list(genes)              # [{uid, start, end, strand, key}]
        self.total_len = total_len
        self.types_present = list(types_present)
        defn = records[0][2] if records else ""
        defn = defn or (records[0][0] if records else "genome")
        self.title = "%s  —  %s bp · %d features · %d contig(s)" % (
            defn, format_bp(total_len), len(genes), len(records),
        )

    # -- drawing ---------------------------------------------------------

    def draw(self, ctx):
        base = ctx.constraints
        width = base.x.provided_value() if base.x is not None else None
        height = base.y.provided_value() if base.y is not None else None
        if width is None or height is None:
            return dex.DrawResult.Complete(region=None)

        x0 = base.pos.x + MARGIN
        y0 = base.pos.y + MARGIN
        plot_w = width - 2.0 * MARGIN
        plot_h = height - 2.0 * MARGIN

        self._text(ctx, self.title, x0, y0, TITLE_FONT, INK)

        top = y0 + TITLE_H
        avail_h = plot_h - TITLE_H - LEGEND_H
        if avail_h < ROW_H or self.total_len <= 0 or plot_w <= RULER_GUTTER:
            self._text(ctx, "no genome to draw", x0, top, COORD_FONT, FAINT)
            return self._done(base, width, height)

        n_rows = max(1, int(avail_h // ROW_H))
        bp_per_row = max(1, int(math.ceil(self.total_len / n_rows)))
        n_rows = int(math.ceil(self.total_len / bp_per_row))
        track_x = x0 + RULER_GUTTER
        track_w = plot_w - RULER_GUTTER
        scale = track_w / bp_per_row

        coord_font = dex.Font.proportional(COORD_FONT)
        wrap = dex.TextWrap.singleline()

        # The rows: a centre line each, and the bp coordinate it starts at.
        for r in range(n_rows):
            row_bp0 = r * bp_per_row
            line_y = top + r * ROW_H + BAND + LINE_HALF
            row_len = min(bp_per_row, self.total_len - row_bp0)
            self._line(ctx, [(track_x, line_y), (track_x + row_len * scale, line_y)],
                       LINE_INK, 1.0)
            m = ctx.measure_text(format_bp(row_bp0 + 1), coord_font, wrap)
            self._text(ctx, format_bp(row_bp0 + 1),
                       track_x - 6.0 - m.width, line_y - m.height / 2.0,
                       COORD_FONT, FAINT)

        # The genes: each drawn inspectable into the box its span maps to.
        for g in self.genes:
            r = g["start"] // bp_per_row
            if r >= n_rows:
                continue
            row_bp0 = r * bp_per_row
            seg_end = min(g["end"], row_bp0 + bp_per_row)
            gx0 = track_x + (g["start"] - row_bp0) * scale
            gx1 = track_x + (seg_end - row_bp0) * scale
            if gx1 - gx0 < GENE_MIN_PX:
                gx1 = gx0 + GENE_MIN_PX
            line_y = top + r * ROW_H + BAND + LINE_HALF
            if g["strand"] >= 0:
                (gy0, gy1) = (line_y - LINE_HALF - BAND, line_y - LINE_HALF)
            else:
                (gy0, gy1) = (line_y + LINE_HALF, line_y + LINE_HALF + BAND)
            ctx.draw_inspectable_node(g["uid"], self._box(gx0, gy0, gx1 - gx0, gy1 - gy0))

        # Contig boundaries, where there is more than one.
        for off in self.offsets[1:]:
            r = off // bp_per_row
            if r >= n_rows:
                continue
            bx = track_x + (off - r * bp_per_row) * scale
            row_top = top + r * ROW_H
            self._line(ctx, [(bx, row_top), (bx, row_top + 2.0 * BAND + 2.0 * LINE_HALF)],
                       CONTIG_INK, 1.0)

        self._legend(ctx, x0, top + n_rows * ROW_H + 4.0, plot_w)
        return self._done(base, width, height)

    def _legend(self, ctx, x, y, width):
        """A swatch and name per feature type present, left to right."""
        font = dex.Font.proportional(LEGEND_FONT)
        wrap = dex.TextWrap.singleline()
        sw = 11.0
        for key in self.types_present:
            m = ctx.measure_text(key, font, wrap)
            if x + sw + 4.0 + m.width > base_right(x, width):
                break
            self._polygon(ctx, [(x, y), (x + sw, y), (x + sw, y + sw), (x, y + sw)],
                          FEATURE_COLORS.get(key, FAINT))
            self._text(ctx, key, x + sw + 4.0, y + (sw - m.height) / 2.0, LEGEND_FONT, INK)
            x += sw + 6.0 + m.width + 16.0

    # -- draw helpers ----------------------------------------------------

    def _abs(self):
        return dex.DrawConstraints(
            pos=dex.ScreenPos.new(0.0, 0.0),
            x=None, y=None, wrap=None, should_clip=False,
        )

    def _box(self, x, y, w, h):
        return dex.DrawConstraints(
            pos=dex.ScreenPos.new(x, y),
            x=dex.AxisConstraint.Exactly(w),
            y=dex.AxisConstraint.Exactly(h),
            wrap=None, should_clip=False,
        )

    def _polygon(self, ctx, points, rgb):
        ctx.draw_node(
            dex.Path.polygon(
                [dex.Vector.new(x, y) for (x, y) in points],
                dex.Color.rgb(*rgb), dex.Stroke.none(),
            ),
            self._abs(),
        )

    def _line(self, ctx, points, rgb, width):
        ctx.draw_node(
            dex.Path.polyline(
                [dex.Vector.new(x, y) for (x, y) in points],
                dex.Stroke.new(width, dex.Color.rgb(*rgb)),
            ),
            self._abs(),
        )

    def _text(self, ctx, text, x, y, size, rgb):
        label = dex.Label.new(text)
        label.font = dex.Font.proportional(size)
        label.color = dex.Color.rgb(*rgb)
        ctx.draw_node(
            label,
            dex.DrawConstraints(
                pos=dex.ScreenPos.new(x, y),
                x=None, y=None, wrap=None, should_clip=False,
            ),
        )

    def _done(self, base, width, height):
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(width, height))
        )

    # -- messages --------------------------------------------------------

    def type_name(self):
        return "A Genome Explorer"

    def owned_nodes(self):
        return [g["uid"] for g in self.genes]

    def on_delete(self, ctx):
        for g in self.genes:
            ctx.workspace.delete_node(g["uid"])

    def build_inspector(self, ctx):
        return None


def base_right(x0, width):
    """The right edge the legend may run to (the plot's right margin)."""
    return x0 + width


# ======================================================================
# Build and transform
# ======================================================================


def build(ws, genbank_text):
    """Parse `genbank_text` and build a `Gene` node for every drawn feature."""
    records = parse_genbank(genbank_text)
    genes = []
    offsets = []
    recmeta = []
    types_present = []
    offset = 0
    for rec in records:
        length = rec["length"]
        feats = rec["features"]
        if not length:
            ends = [parse_location(loc)[1] for (k, loc, q) in feats if parse_location(loc)]
            length = max(ends) if ends else 0
        offsets.append(offset)
        recmeta.append((rec["name"], length, rec["definition"]))
        for (key, loc, quals) in feats:
            if key not in FEATURE_COLORS:
                continue
            parsed = parse_location(loc)
            if parsed is None:
                continue
            (s, e, strand) = parsed
            gene = Gene(
                key,
                quals.get("gene", ""),
                quals.get("locus_tag", ""),
                quals.get("product", ""),
                s, e, strand,
                FEATURE_COLORS[key],
                rec["name"],
            )
            uid = ws.insert_node_dyn(gene)
            genes.append({"uid": uid, "start": offset + s, "end": offset + e,
                          "strand": strand, "key": key})
            if key not in types_present:
                types_present.append(key)
        offset += length
    return GenomeExplorer(recmeta, offsets, genes, offset, types_present)


def transform():
    """The genome map of the wired `genebank_data` string."""
    text = genebank_data if "genebank_data" in globals() else None
    if not text:
        # Fall back to any string global, so a differently-named input still works.
        for value in globals().values():
            if isinstance(value, str) and "FEATURES" in value and "LOCUS" in value:
                text = value
                break
    if not text:
        raise ValueError("wire a GenBank (.gbff) string into this transform as `genebank_data`")
    return build(dex.ws, text)
