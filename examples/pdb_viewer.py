"""A drag-to-spin protein viewer from a PDB file.

Give the transform a `.pdb` file's text as `pdb_data` and it draws the structure
as a backbone trace you rotate by dragging left and right — not a 3D renderer,
just enough projection to turn a coordinate file into something you can turn over
in your hand.

How the spin works, and the one interesting part: a drawing node cannot read the
pointer on its own, so rotation rides an **`InteractionBox`** — a node that
senses gestures over the box it is given and answers `WasDragged` with the
frame's drag delta. Each frame the viewer draws that sensor across the whole
viewport, asks it how far the pointer dragged, and adds the horizontal part to a
stored angle. The molecule is then projected at that angle. Drag right, it turns
right; let go, it holds. Nothing animates on its own — the picture only moves
while you move it.

Everything else is small: the CA atoms of each chain, read from the `ATOM`
records, are rotated about the vertical, tilted a little for a three-quarter
view, and projected flat. Depth is kept only to sort the strokes back-to-front
and to fade the far side, which is all the cue a trace needs to read as a shape
rather than a scribble. Ligand atoms (`HETATM`, water dropped) ride along as
dots in their element colours.

The backbone is `dex.Path.polyline` per residue step, and the whole thing is
repainted every frame — a protein is a few hundred residues, so there is no need
for the caching the big plots use.
"""

import math

# ======================================================================
# Colours
# ======================================================================

# CPK element colours for ligand atoms.
CPK = {
    "C": (110, 116, 124),
    "N": (72, 112, 196),
    "O": (204, 84, 78),
    "S": (214, 188, 78),
    "P": (222, 148, 78),
    "H": (222, 224, 228),
    "FE": (204, 120, 60),
    "MG": (90, 176, 120),
    "ZN": (130, 132, 176),
    "CA": (90, 168, 150),
    "NA": (150, 110, 190),
    "CL": (108, 176, 110),
}
CPK_DEFAULT = (188, 120, 168)

INK = (58, 62, 70)
FAINT = (120, 126, 136)
FAR = (238, 240, 244)     # the colour the far side fades toward

# ======================================================================
# Tuning
# ======================================================================

MARGIN = 16.0
TITLE_H = 24.0
LEGEND_H = 22.0
TILT = 0.38               # fixed lean, radians, so it is not seen edge-on
DRAG_SENS = 0.012         # radians of turn per point of horizontal drag
LINE_W = 2.4              # backbone weight at the near edge
DOT_R = 3.2               # ligand-atom radius at the near edge

TITLE_FONT = 13.0
LEGEND_FONT = 10.0

# ======================================================================
# PDB parsing
# ======================================================================


def _f(line, a, b):
    try:
        return float(line[a:b])
    except (ValueError, IndexError):
        return None


def parse_pdb(text):
    """Backbone, ligands and a title from `text`.

    Reads the fixed columns of the `ATOM`/`HETATM` records — only the first
    model of an NMR ensemble, and no water. Returns the CA atoms per chain in
    order, the ligand atoms, every atom (for centring), and a title.
    """
    chains = {}            # chain id -> [(x, y, z)] of its CA atoms, in order
    order = []             # chain ids, first-seen
    hets = []              # [(element, (x, y, z))]
    everything = []        # [(x, y, z)] over all atoms, for the centroid
    title_parts = []
    header = ""
    ended = False

    for line in text.splitlines():
        rec = line[:6].strip()
        if rec == "TITLE":
            title_parts.append(line[10:80].strip())
        elif rec == "HEADER":
            header = line[10:50].strip()
        elif rec == "ENDMDL":
            ended = True   # keep only the first model
        elif rec == "ATOM" and not ended:
            (x, y, z) = (_f(line, 30, 38), _f(line, 38, 46), _f(line, 46, 54))
            if x is None:
                continue
            everything.append((x, y, z))
            if line[12:16].strip() == "CA":
                chain = line[21:22] or " "
                if chain not in chains:
                    chains[chain] = []
                    order.append(chain)
                chains[chain].append((x, y, z))
        elif rec == "HETATM" and not ended:
            if line[17:20].strip() == "HOH":
                continue
            (x, y, z) = (_f(line, 30, 38), _f(line, 38, 46), _f(line, 46, 54))
            if x is None:
                continue
            everything.append((x, y, z))
            elem = (line[76:78].strip() or line[12:14].strip()).upper()
            hets.append((elem, (x, y, z)))

    title = " ".join(p for p in title_parts if p) or header or "structure"
    return title, order, chains, hets, everything


# ======================================================================
# Colour helpers
# ======================================================================


def hsv_rgb(h, s, v):
    i = int(h * 6.0)
    f = h * 6.0 - i
    p, q, t = v * (1 - s), v * (1 - s * f), v * (1 - s * (1 - f))
    (r, g, b) = [(v, t, p), (q, v, p), (p, v, t), (p, q, v), (t, p, v), (v, p, q)][i % 6]
    return (int(r * 255), int(g * 255), int(b * 255))


def lerp_rgb(a, b, t):
    t = max(0.0, min(t, 1.0))
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def octagon(cx, cy, r):
    return [
        (cx + r * math.cos(k * math.pi / 4.0), cy + r * math.sin(k * math.pi / 4.0))
        for k in range(8)
    ]


# ======================================================================
# The viewer
# ======================================================================


class Protein:
    """A backbone trace you turn by dragging. Owns its drag sensor.

    Holds only coordinates (picklable) and the sensor's id; the rotation angle
    is view state, mutated in `draw` as the drag comes in and kept across frames
    on the node itself.
    """

    def __init__(self, title, order, chains, hets, allatoms, sensor):
        self.title = title
        self.order = list(order)
        self.chains = {c: list(pts) for (c, pts) in chains.items()}
        self.hets = list(hets)
        self.sensor = sensor
        self.angle = 0.0
        # Centre and fit-radius from every atom, so the whole thing sits in the
        # box and rotation (which preserves distance) never clips.
        pts = allatoms or [p for c in self.chains.values() for p in c]
        if pts:
            self.center = tuple(sum(p[i] for p in pts) / len(pts) for i in range(3))
            self.radius = max(
                1.0,
                max(math.dist(p, self.center) for p in pts),
            )
        else:
            self.center = (0.0, 0.0, 0.0)
            self.radius = 1.0
        self.chain_ink = {
            c: hsv_rgb((i / max(len(order), 1)) % 1.0, 0.52, 0.66)
            for (i, c) in enumerate(order)
        }

    # -- drawing ---------------------------------------------------------

    def draw(self, ctx):
        base = ctx.constraints
        width = base.x.provided_value() if base.x is not None else None
        height = base.y.provided_value() if base.y is not None else None
        if width is None or height is None:
            return dex.DrawResult.Complete(region=None)

        # The drag sensor covers the whole box. Drawn first, so its cache holds
        # this frame's drag by the time we ask.
        ctx.draw_node(self.sensor, self._box(base.pos.x, base.pos.y, width, height))
        drag = ctx.node.workspace.send_request(self.sensor, dex.WasDragged())
        if drag is not None:
            self.angle += drag.x * DRAG_SENS

        self._text(ctx, self.title, base.pos.x + MARGIN, base.pos.y + MARGIN,
                   TITLE_FONT, INK)

        avail = min(width, height - TITLE_H - LEGEND_H) / 2.0 - MARGIN
        if avail <= 0.0:
            return self._done(base, width, height)
        scale = avail / self.radius
        ox = base.pos.x + width / 2.0
        oy = base.pos.y + TITLE_H + (height - TITLE_H) / 2.0

        (ay, ax) = (self.angle, TILT)
        (ca, sa) = (math.cos(ay), math.sin(ay))
        (cb, sb) = (math.cos(ax), math.sin(ax))
        (cx, cy, cz) = self.center

        def project(p):
            dx, dy, dz = p[0] - cx, p[1] - cy, p[2] - cz
            dx, dz = dx * ca + dz * sa, -dx * sa + dz * ca   # about the vertical
            dy, dz = dy * cb - dz * sb, dy * sb + dz * cb     # the fixed tilt
            return (ox + dx * scale, oy - dy * scale, dz)

        # Everything that will be drawn, tagged with its depth, so it can be
        # sorted back-to-front and faded by distance.
        draws = []
        for chain in self.order:
            ink = self.chain_ink[chain]
            proj = [project(p) for p in self.chains[chain]]
            for i in range(len(proj) - 1):
                (a, b) = (proj[i], proj[i + 1])
                draws.append(((a[2] + b[2]) / 2.0, "line", (a, b, ink)))
        for (elem, p) in self.hets:
            q = project(p)
            draws.append((q[2], "dot", (q, CPK.get(elem, CPK_DEFAULT))))
        if not draws:
            return self._done(base, width, height)

        zlo = min(d[0] for d in draws)
        zhi = max(d[0] for d in draws)
        span = (zhi - zlo) or 1.0
        draws.sort(key=lambda d: d[0])   # far first

        for (z, kind, payload) in draws:
            t = (z - zlo) / span
            if kind == "line":
                (a, b, ink) = payload
                shade = lerp_rgb(lerp_rgb(ink, FAR, 0.7), ink, t)
                self._line(ctx, [(a[0], a[1]), (b[0], b[1])], shade,
                           LINE_W * (0.5 + 0.7 * t))
            else:
                (q, ink) = payload
                shade = lerp_rgb(lerp_rgb(ink, FAR, 0.6), ink, t)
                self._polygon(ctx, octagon(q[0], q[1], DOT_R * (0.6 + 0.6 * t)), shade)

        self._legend(ctx, base.pos.x + MARGIN,
                     base.pos.y + height - LEGEND_H + 4.0, width)
        return self._done(base, width, height)

    def _legend(self, ctx, x, y, width):
        font = dex.Font.proportional(LEGEND_FONT)
        wrap = dex.TextWrap.singleline()
        sw = 11.0
        for chain in self.order:
            name = "chain %s" % (chain.strip() or "?")
            m = ctx.measure_text(name, font, wrap)
            self._polygon(ctx, [(x, y), (x + sw, y), (x + sw, y + sw), (x, y + sw)],
                          self.chain_ink[chain])
            self._text(ctx, name, x + sw + 4.0, y + (sw - m.height) / 2.0,
                       LEGEND_FONT, INK)
            x += sw + 6.0 + m.width + 14.0

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

    def _line(self, ctx, pts, rgb, width):
        ctx.draw_node(
            dex.Path.polyline(
                [dex.Vector.new(px, py) for (px, py) in pts],
                dex.Stroke.new(width, dex.Color.rgb(*rgb)),
            ),
            self._abs(),
        )

    def _polygon(self, ctx, pts, rgb):
        ctx.draw_node(
            dex.Path.polygon(
                [dex.Vector.new(px, py) for (px, py) in pts],
                dex.Color.rgb(*rgb), dex.Stroke.none(),
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
        return "A PDB Viewer"

    def owned_nodes(self):
        return [self.sensor]

    def on_delete(self, ctx):
        ctx.workspace.delete_node(self.sensor)

    def build_inspector(self, ctx):
        return None


# ======================================================================
# Build and transform
# ======================================================================


def build(ws, pdb_text):
    """Parse `pdb_text` and build the viewer with its drag sensor."""
    (title, order, chains, hets, allatoms) = parse_pdb(pdb_text)
    sensor = ws.insert_node_dyn(dex.InteractionBox.sensing(False, False, True))
    return Protein(title, order, chains, hets, allatoms, sensor)


def transform():
    """The drag-to-spin viewer of the wired `pdb_data` string."""
    text = pdb_data if "pdb_data" in globals() else None
    if not text:
        for value in globals().values():
            if isinstance(value, str) and "\nATOM" in ("\n" + value):
                text = value
                break
    if not text:
        raise ValueError("wire a PDB (.pdb) string into this transform as `pdb_data`")
    return build(dex.ws, text)
