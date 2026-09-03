"""An assembly QC plot: concentric tracks around a set of contigs.

The other circos idiom. `circos.py` is the chord kind — arcs joined by ribbons
— and this is the one you get out of an assembly report, where the ring is the
assembly itself and each track is a question asked of it.

Going inward:

  * **Contigs** — the ideogram, one arc per contig, sized by length and
    ordered longest first. Ticks every 2 Mb.
  * **GC content** — a band around the assembly's mean, sampled per 1 Mb bin.
    Gridlines at 30/40/50/60 %.
  * **Read depth** — a bar per bin against the median, with anything past
    twice the median drawn in warning colour: the shape a collapsed repeat
    makes.
  * **Gaps** — runs of `N` where the scaffolder joined without sequence.
  * **Repeat links** — the only ribbons here, three of them, joining regions
    that share a repeat family.

The data is hardcoded and made up, but it is made up to *say* something, which
the first version's ribbons were not. Read it and the assembly has three
problems: `ctg7` sits about fifteen points above everything else in GC and is
likely a contaminant; `ctg8` runs at double the median depth and is likely a
collapsed repeat; and `ctg2` and `ctg4` carry scaffold gaps. Those are the
three things this plot exists to make obvious at a glance.

Everything is `dex.Path.polygon`. A track band is an annulus sector with one
edge driven by data; the GC band is the same with both edges driven by it.
"""

import math

# ======================================================================
# Data
# ======================================================================

# Bin width along a contig, in bases. Every binned track below is sampled at
# this pitch, so a bin is the same span on every track.
BIN = 1_000_000

# (name, length in bases). Longest first, as an assembly is conventionally
# reported.
CONTIGS = [
    ("ctg1", 8_400_000),
    ("ctg2", 6_900_000),
    ("ctg3", 5_600_000),
    ("ctg4", 4_800_000),
    ("ctg5", 3_900_000),
    ("ctg6", 3_100_000),
    ("ctg7", 2_400_000),
    ("ctg8", 1_600_000),
]

# GC fraction per bin. `ctg7` is the odd one out — fifteen points high across
# its whole length, which is what a contaminant contig looks like.
GC = {
    "ctg1": [0.44, 0.46, 0.43, 0.47, 0.45, 0.42, 0.46, 0.44, 0.45],
    "ctg2": [0.47, 0.45, 0.44, 0.46, 0.48, 0.45, 0.43],
    "ctg3": [0.42, 0.44, 0.46, 0.43, 0.45, 0.44],
    "ctg4": [0.45, 0.47, 0.44, 0.46, 0.43],
    "ctg5": [0.46, 0.43, 0.45, 0.44],
    "ctg6": [0.44, 0.46, 0.45, 0.42],
    "ctg7": [0.61, 0.63, 0.60],
    "ctg8": [0.45, 0.44],
}

# Mean read depth per bin. `ctg8` runs at roughly twice the median across both
# its bins: a collapsed repeat, two copies piled into one.
DEPTH = {
    "ctg1": [38.0, 44.0, 46.0, 45.0, 47.0, 43.0, 45.0, 44.0, 31.0],
    "ctg2": [42.0, 46.0, 45.0, 44.0, 47.0, 45.0, 36.0],
    "ctg3": [44.0, 45.0, 43.0, 46.0, 44.0, 40.0],
    "ctg4": [45.0, 43.0, 46.0, 44.0, 39.0],
    "ctg5": [46.0, 44.0, 45.0, 42.0],
    "ctg6": [43.0, 45.0, 44.0, 41.0],
    "ctg7": [22.0, 24.0, 21.0],
    "ctg8": [92.0, 88.0],
}

# The depth every other track is read against. Stated rather than derived, the
# way an assembly report quotes the coverage it was assembled at.
MEDIAN_DEPTH = 45.0

# Runs of `N`: (contig, start, end) in bases. Where the scaffolder joined two
# contigs on linkage alone and put no sequence between them.
GAPS = [
    ("ctg2", 3_050_000, 3_260_000),
    ("ctg2", 5_400_000, 5_480_000),
    ("ctg4", 1_700_000, 1_950_000),
]

# (contig, start, end, contig, start, end) — regions sharing a repeat family.
# Three, not thirty: on a track plot the ribbons are an annotation, not the
# subject.
REPEATS = [
    ("ctg1", 2_100_000, 2_400_000, "ctg5", 900_000, 1_200_000),
    ("ctg3", 4_200_000, 4_500_000, "ctg6", 400_000, 700_000),
    ("ctg2", 900_000, 1_150_000, "ctg4", 3_600_000, 3_850_000),
]

# ======================================================================
# Geometry
# ======================================================================

CONTIG_GAP = 0.028
START_ANGLE = -math.pi / 2

# Radii, as fractions of the outer radius. Outermost first.
R_RING_OUT = 1.00
R_RING_IN = 0.94
R_TICKS = 0.915
R_GC_OUT = 0.89
R_GC_IN = 0.68
R_DEPTH_OUT = 0.645
R_DEPTH_IN = 0.44
R_GAP_OUT = 0.415
R_GAP_IN = 0.385
R_LINK = 0.36

# What each track's radius spans.
GC_MIN, GC_MAX = 0.28, 0.68
GC_GRID = [0.30, 0.40, 0.50, 0.60]
# Depth is read against the median, so the track spans a multiple of it.
DEPTH_MAX = 2.4
# Past this, a bin is called out rather than just drawn tall.
DEPTH_WARN = 2.0

TICK_EVERY = 2_000_000
TICK_WIDTH = 0.0035

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


def curve_steps(p0, control, p1):
    """How many samples a quadratic deserves, by how far it bows.

    A quadratic's greatest departure from the straight line between its ends is
    half the distance from that line's midpoint to the control point, and
    flattening error falls as the square of the step count.
    """
    mid = ((p0[0] + p1[0]) / 2.0, (p0[1] + p1[1]) / 2.0)
    bow = math.dist(mid, control) * 0.5
    return max(2, int(math.sqrt(bow / CURVE_TOLERANCE)) + 2)
# How far a link's control point sits from the centre.
LINK_BOW = 0.18

NAME_FONT = 12.0
AXIS_FONT = 9.0
LABEL_GAP = 9.0

INK = (58, 62, 70)
AXIS_INK = (188, 194, 204)
TRACK_BG = (240, 242, 246)
CONTIG_INK = (128, 140, 158)
GC_INK = (92, 132, 176)
DEPTH_INK = (118, 152, 118)
WARN_INK = (206, 124, 88)
GAP_INK = (198, 96, 96)
LINK_INK = (150, 132, 186)


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


def ring(cx, cy, r_in, r_out):
    """A full annulus, as the several polygons it has to be. See `sectors`."""
    return sectors(cx, cy, r_in, r_out, 0.0, 2.0 * math.pi)


def lerp(a, b, t):
    return a + (b - a) * t


def quadratic(p0, control, p1):
    """Points along the quadratic Bézier `p0` -> `p1` bending toward `control`."""
    steps = curve_steps(p0, control, p1)
    out = []
    for i in range(steps):
        t = i / (steps - 1)
        u = 1.0 - t
        out.append(
            (
                u * u * p0[0] + 2 * u * t * control[0] + t * t * p1[0],
                u * u * p0[1] + 2 * u * t * control[1] + t * t * p1[1],
            )
        )
    return out


def scaled(value, lo, hi, r_in, r_out):
    """Where `value` sits on a track spanning `r_in`..`r_out`."""
    if hi <= lo:
        return r_in
    return lerp(r_in, r_out, max(0.0, min((value - lo) / (hi - lo), 1.0)))


def contig_angles(contigs, gap=CONTIG_GAP, start=START_ANGLE):
    """`{name: (a0, a1, length)}` — where each contig sits on the ring."""
    total = sum(length for (_n, length) in contigs)
    if total <= 0:
        return {}
    usable = 2.0 * math.pi - gap * len(contigs)
    spans = {}
    angle = start
    for (name, length) in contigs:
        width = usable * (length / total)
        spans[name] = (angle, angle + width, length)
        angle += width + gap
    return spans


def at(spans, name, base):
    """The angle of position `base` (in bases) along the contig `name`."""
    a0, a1, length = spans[name]
    if length <= 0:
        return a0
    return a0 + (a1 - a0) * (max(0, min(base, length)) / length)


def bin_span(spans, name, index):
    """`(a0, a1)` for bin `index`, clipped to the contig's real end.

    The last bin of a contig is usually a part bin, and it gets the narrower
    arc it deserves rather than a full one.
    """
    _a0, _a1, length = spans[name]
    return (
        at(spans, name, index * BIN),
        at(spans, name, min((index + 1) * BIN, length)),
    )


# ======================================================================
# The node
# ======================================================================


class AssemblyPlot:
    """An assembly's contigs and what was measured along them.

    Holds plain tuples and floats rather than `dex` values, which keeps it
    picklable — the same rule the other Python examples follow.
    """

    def __init__(self, contigs=None, gc=None, depth=None, gaps=None, repeats=None):
        self.contigs = list(CONTIGS if contigs is None else contigs)
        self.gc = dict(GC if gc is None else gc)
        self.depth = dict(DEPTH if depth is None else depth)
        self.gaps = list(GAPS if gaps is None else gaps)
        self.repeats = list(REPEATS if repeats is None else repeats)

    # -- drawing ---------------------------------------------------------

    def draw(self, ctx):
        base = ctx.constraints
        width = base.x.provided_value() if base.x is not None else None
        height = base.y.provided_value() if base.y is not None else None
        if width is None or height is None or not self.contigs:
            # A circle has no natural size; without a box there is nothing to fill.
            return dex.DrawResult.Complete(region=None)

        radius = min(width, height) / 2.0 - LABEL_GAP - self._label_reach(ctx)
        if radius <= 0.0:
            return dex.DrawResult.Complete(region=None)
        cx = base.pos.x + width / 2.0
        cy = base.pos.y + height / 2.0
        spans = contig_angles(self.contigs)

        # Links first, so every track paints over them; then outermost in.
        self._links(ctx, cx, cy, radius, spans)
        self._contigs(ctx, cx, cy, radius, spans)
        self._gc(ctx, cx, cy, radius, spans)
        self._depth(ctx, cx, cy, radius, spans)
        self._gaps(ctx, cx, cy, radius, spans)
        self._axis(ctx, cx, cy, radius)
        self._labels(ctx, cx, cy, radius, spans)

        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(
                base.pos, dex.Vector.new(width, height)
            )
        )

    def _origin(self):
        """Constraints for a path whose points are already absolute."""
        return dex.DrawConstraints(
            pos=dex.ScreenPos.new(0.0, 0.0),
            x=None,
            y=None,
            wrap=None,  # NotAllowed
            should_clip=False,
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

    def _sectors(self, ctx, cx, cy, r_in, r_out, a0, a1, rgb, alpha=255):
        """A filled band, split so its fill cannot bridge the hole."""
        for part in sectors(cx, cy, r_in, r_out, a0, a1):
            self._polygon(ctx, part, rgb, alpha)

    def _ring(self, ctx, cx, cy, r_in, r_out, rgb, alpha=255):
        """A full annulus, in one colour — as the several polygons it must be."""
        for part in ring(cx, cy, r_in, r_out):
            self._polygon(ctx, part, rgb, alpha)

    def _contigs(self, ctx, cx, cy, radius, spans):
        """One arc per contig, with a tick every `TICK_EVERY` bases."""
        for (name, length) in self.contigs:
            a0, a1, _len = spans[name]
            self._polygon(
                ctx,
                sector(cx, cy, radius * R_RING_IN, radius * R_RING_OUT, a0, a1),
                CONTIG_INK,
            )
            base = TICK_EVERY
            while base < length:
                angle = at(spans, name, base)
                self._polygon(
                    ctx,
                    sector(
                        cx,
                        cy,
                        radius * R_TICKS,
                        radius * R_RING_IN,
                        angle - TICK_WIDTH,
                        angle + TICK_WIDTH,
                    ),
                    INK,
                    alpha=140,
                )
                base += TICK_EVERY

    def _gc(self, ctx, cx, cy, radius, spans):
        """GC per bin, as a band hanging from the track's baseline."""
        r_in = radius * R_GC_IN
        r_out = radius * R_GC_OUT
        self._ring(ctx, cx, cy, r_in, r_out, TRACK_BG)
        for value in GC_GRID:
            r = scaled(value, GC_MIN, GC_MAX, r_in, r_out)
            self._ring(ctx, cx, cy, r - 0.5, r + 0.5, AXIS_INK)

        # A column per bin rather than a smoothed curve: the bins are the
        # measurement, and a contaminant contig should look like a step.
        for (name, _length) in self.contigs:
            values = self.gc.get(name) or []
            for (i, value) in enumerate(values):
                (b0, b1) = bin_span(spans, name, i)
                r = scaled(value, GC_MIN, GC_MAX, r_in, r_out)
                self._polygon(ctx, sector(cx, cy, r_in, r, b0, b1), GC_INK, alpha=210)

    def _depth(self, ctx, cx, cy, radius, spans):
        """Read depth per bin, against the median, flagged where it doubles."""
        r_in = radius * R_DEPTH_IN
        r_out = radius * R_DEPTH_OUT
        self._ring(ctx, cx, cy, r_in, r_out, TRACK_BG)
        # The median, so a bar is read as a ratio rather than a height.
        r_med = scaled(1.0, 0.0, DEPTH_MAX, r_in, r_out)
        self._ring(ctx, cx, cy, r_med - 0.5, r_med + 0.5, AXIS_INK)

        for (name, _length) in self.contigs:
            values = self.depth.get(name) or []
            for (i, value) in enumerate(values):
                (b0, b1) = bin_span(spans, name, i)
                ratio = value / MEDIAN_DEPTH if MEDIAN_DEPTH > 0 else 0.0
                r = scaled(ratio, 0.0, DEPTH_MAX, r_in, r_out)
                inset = (b1 - b0) * 0.08
                self._polygon(
                    ctx,
                    sector(cx, cy, r_in, r, b0 + inset, b1 - inset),
                    WARN_INK if ratio >= DEPTH_WARN else DEPTH_INK,
                )

    def _gaps(self, ctx, cx, cy, radius, spans):
        """Runs of `N`: a thin ring, marked only where the sequence is missing."""
        r_in = radius * R_GAP_IN
        r_out = radius * R_GAP_OUT
        self._ring(ctx, cx, cy, r_in, r_out, TRACK_BG)
        for (name, start, end) in self.gaps:
            if name not in spans:
                continue
            self._polygon(
                ctx,
                sector(
                    cx, cy, r_in, r_out, at(spans, name, start), at(spans, name, end)
                ),
                GAP_INK,
            )

    def _links(self, ctx, cx, cy, radius, spans):
        """Shared repeat families, as ribbons through the middle."""
        r = radius * R_LINK
        for (src, s0, s1, dst, d0, d1) in self.repeats:
            if src not in spans or dst not in spans:
                continue
            (a0, a1) = (at(spans, src, s0), at(spans, src, s1))
            (b0, b1) = (at(spans, dst, d0), at(spans, dst, d1))
            control = polar(cx, cy, r * LINK_BOW, (a1 + b0) / 2.0 + math.pi)
            back = polar(cx, cy, r * LINK_BOW, (b1 + a0) / 2.0 + math.pi)
            points = (
                arc(cx, cy, r, a0, a1)
                + quadratic(polar(cx, cy, r, a1), control, polar(cx, cy, r, b0))
                + arc(cx, cy, r, b0, b1)
                + quadratic(polar(cx, cy, r, b1), back, polar(cx, cy, r, a0))
            )
            self._polygon(ctx, points, LINK_INK, alpha=120)

    def _axis(self, ctx, cx, cy, radius):
        """Caption the GC gridlines once, at the top of the circle."""
        r_in = radius * R_GC_IN
        r_out = radius * R_GC_OUT
        font = dex.Font.proportional(AXIS_FONT)
        wrap = dex.TextWrap.singleline()
        for value in GC_GRID:
            text = f"{int(round(value * 100))}%"
            metrics = ctx.measure_text(text, font, wrap)
            r = scaled(value, GC_MIN, GC_MAX, r_in, r_out)
            (x, y) = polar(cx, cy, r, START_ANGLE - CONTIG_GAP / 2.0)
            label = dex.Label.new(text)
            label.font = font
            label.color = dex.Color.rgb(*INK)
            ctx.draw_node(
                label,
                dex.DrawConstraints(
                    pos=dex.ScreenPos.new(x + 4.0, y - metrics.height / 2.0),
                    x=None,
                    y=None,
                    wrap=None,  # NotAllowed
                    should_clip=False,
                ),
            )

    def _label_reach(self, ctx):
        """How far past the ring the longest contig name reaches."""
        font = dex.Font.proportional(NAME_FONT)
        wrap = dex.TextWrap.singleline()
        reach = 0.0
        for (name, _length) in self.contigs:
            metrics = ctx.measure_text(name, font, wrap)
            reach = max(reach, metrics.width, metrics.height)
        return reach

    def _labels(self, ctx, cx, cy, radius, spans):
        """Each contig's name, centred on it and clear of the ring."""
        font = dex.Font.proportional(NAME_FONT)
        wrap = dex.TextWrap.singleline()
        for (name, _length) in self.contigs:
            a0, a1, _len = spans[name]
            middle = (a0 + a1) / 2.0
            metrics = ctx.measure_text(name, font, wrap)
            (x, y) = polar(cx, cy, radius + LABEL_GAP, middle)
            # Pushed out along its own radius, so it clears the ring on
            # whichever side of the circle it is on.
            x += math.cos(middle) * metrics.width / 2.0
            y += math.sin(middle) * metrics.height / 2.0

            label = dex.Label.new(name)
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

    # -- messages --------------------------------------------------------

    def type_name(self):
        return "An Assembly Plot"


def transform():
    """A lambda returning the assembly plot of the hardcoded metrics."""
    return AssemblyPlot()
