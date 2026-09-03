"""A circos-style plot: an ideogram ring, a value track, and chord ribbons.

Everything here is a polygon. There are no arc or ribbon primitives — a circos
plot is just `dex.Path.polygon` with the points worked out in Python:

  * A segment of the ring is an annulus sector: sample the outer arc from one
    angle to the other, then the inner arc back again, and close it.
  * A tick is the same sector, one or two degrees wide.
  * A ribbon is two arcs joined by two Béziers through the middle of the
    circle. The curve is what makes a chord read as a link rather than a
    chord *line*: sampled as a quadratic whose control point sits near the
    centre, so ribbons between neighbours bow gently and ribbons across the
    circle dive through the middle.

Sampling by hand rather than reaching for `dex.Anchor.smooth` is deliberate.
The anchors carry cubic handles, which would draw the same curves in fewer
points — but a ribbon's two edges have to stay parallel along their length, and
that is far easier to guarantee by evaluating both from the same parameter than
by fitting handles to each.

Labels are the one thing that is *not* geometry, so they are placed by asking:
`ctx.measure_text` lays a label out without drawing it, which is what lets a
name be centred on its segment and pushed clear of the ring. Estimating from a
character width would put every label slightly wrong and long ones very wrong.

The data is hardcoded in `SEGMENTS` and `LINKS`. Swap it for anything with the
same shape — a genome, a trade matrix, who emails whom.
"""

import math

# ======================================================================
# Data
# ======================================================================

# (name, size, rgb). Size is in whatever unit the links use; only ratios matter.
SEGMENTS = [
    ("chr1", 249.0, (188, 96, 96)),
    ("chr2", 243.0, (206, 140, 88)),
    ("chr3", 198.0, (198, 178, 92)),
    ("chr4", 191.0, (128, 176, 112)),
    ("chr5", 181.0, (96, 168, 160)),
    ("chr6", 171.0, (100, 148, 200)),
    ("chr7", 159.0, (138, 122, 198)),
    ("chr8", 146.0, (184, 112, 168)),
]

# (from_segment, from_start, from_end, to_segment, to_start, to_end).
# Spans, not points: a link joins a *region* to a region, which is what gives
# a ribbon its width at each end.
LINKS = [
    ("chr1", 20.0, 46.0, "chr5", 90.0, 122.0),
    ("chr1", 150.0, 172.0, "chr3", 30.0, 54.0),
    ("chr1", 200.0, 214.0, "chr8", 60.0, 78.0),
    ("chr2", 12.0, 40.0, "chr6", 100.0, 130.0),
    ("chr2", 96.0, 118.0, "chr4", 140.0, 166.0),
    ("chr2", 190.0, 208.0, "chr7", 20.0, 36.0),
    ("chr3", 110.0, 138.0, "chr7", 92.0, 120.0),
    ("chr3", 160.0, 176.0, "chr5", 10.0, 28.0),
    ("chr4", 20.0, 44.0, "chr8", 100.0, 128.0),
    ("chr4", 80.0, 96.0, "chr6", 30.0, 44.0),
    ("chr5", 140.0, 164.0, "chr8", 12.0, 34.0),
    ("chr6", 150.0, 168.0, "chr7", 138.0, 156.0),
]

# One bar per bin, per segment, drawn in the track inside the ring. Values are
# 0..1 and are the bar's height as a fraction of the track's depth.
TRACK = {
    "chr1": [0.35, 0.62, 0.28, 0.81, 0.44, 0.20, 0.55, 0.72],
    "chr2": [0.22, 0.48, 0.90, 0.36, 0.61, 0.29, 0.44, 0.18],
    "chr3": [0.58, 0.31, 0.24, 0.67, 0.85, 0.40, 0.26],
    "chr4": [0.44, 0.70, 0.52, 0.19, 0.33, 0.78, 0.41],
    "chr5": [0.66, 0.25, 0.49, 0.88, 0.30, 0.57],
    "chr6": [0.29, 0.54, 0.73, 0.38, 0.21, 0.63],
    "chr7": [0.47, 0.82, 0.35, 0.59, 0.27],
    "chr8": [0.71, 0.33, 0.50, 0.24, 0.66],
}

# ======================================================================
# Geometry
# ======================================================================

# Gap between neighbouring segments, in radians. Without it the ring reads as
# one unbroken annulus and the segments stop being separate things.
SEGMENT_GAP = 0.035
# Where the ring starts. Straight up, so the first segment leads clockwise
# from twelve o'clock the way a circos plot conventionally does.
START_ANGLE = -math.pi / 2

# Radii, as fractions of the plot's outer radius.
R_RING_OUT = 1.00
R_RING_IN = 0.93
R_TICKS = 0.90
R_TRACK_OUT = 0.88
R_TRACK_IN = 0.70
R_RIBBON = 0.68

# How far a Bézier control point sits from the centre, as a fraction of the
# ribbon radius. Zero would send every ribbon exactly through the middle; this
# leaves short links bowed rather than kinked.
RIBBON_BOW = 0.15

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

TICK_EVERY = 50.0
TICK_WIDTH = 0.004

LABEL_FONT = 12.0
# How far outside the ring a name sits.
LABEL_GAP = 10.0

INK = (60, 64, 72)
TRACK_INK = (120, 128, 140)
TRACK_BG = (238, 240, 244)


def polar(cx, cy, radius, angle):
    """The point at `angle` and `radius` from `(cx, cy)`."""
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


def ribbon(cx, cy, radius, a0, a1, b0, b1):
    """A chord joining the arc `a0`..`a1` to the arc `b0`..`b1`.

    Traced as one closed loop: along the source arc, across to the far side,
    along the target arc, and back. Both crossings bend toward a control point
    near the centre, so the two sides of the ribbon stay parallel.
    """
    control = polar(cx, cy, radius * RIBBON_BOW, (a1 + b0) / 2.0 + math.pi)
    back = polar(cx, cy, radius * RIBBON_BOW, (b1 + a0) / 2.0 + math.pi)
    return (
        arc(cx, cy, radius, a0, a1)
        + quadratic(polar(cx, cy, radius, a1), control, polar(cx, cy, radius, b0))
        + arc(cx, cy, radius, b0, b1)
        + quadratic(polar(cx, cy, radius, b1), back, polar(cx, cy, radius, a0))
    )


def segment_angles(segments, gap=SEGMENT_GAP, start=START_ANGLE):
    """`{name: (a0, a1, size)}` — where each segment sits on the ring.

    The gaps come off the top first, so the remaining angle is shared in
    proportion to size however many segments there are.
    """
    total = sum(size for (_name, size, _rgb) in segments)
    if total <= 0.0:
        return {}
    usable = 2.0 * math.pi - gap * len(segments)
    spans = {}
    angle = start
    for (name, size, _rgb) in segments:
        width = usable * (size / total)
        spans[name] = (angle, angle + width, size)
        angle += width + gap
    return spans


def at(spans, name, offset):
    """The angle of `offset` along the segment `name`."""
    a0, a1, size = spans[name]
    if size <= 0.0:
        return a0
    return a0 + (a1 - a0) * (max(0.0, min(offset, size)) / size)


# ======================================================================
# The node
# ======================================================================


class CircosPlot:
    """A circos plot of `segments` and `links`, drawn into whatever box it gets.

    Holds plain tuples and floats rather than `dex` values, which is what keeps
    it picklable — the same rule the other Python examples follow.
    """

    def __init__(self, segments=None, links=None, track=None):
        self.segments = list(SEGMENTS if segments is None else segments)
        self.links = list(LINKS if links is None else links)
        self.track = dict(TRACK if track is None else track)

    # -- drawing ---------------------------------------------------------

    def draw(self, ctx):
        base = ctx.constraints
        width = base.x.provided_value() if base.x is not None else None
        height = base.y.provided_value() if base.y is not None else None
        if width is None or height is None:
            # A circle has no natural size; without a box there is nothing to
            # fill and nothing sensible to fall back to.
            return dex.DrawResult.Complete(region=None)

        spans = segment_angles(self.segments)
        if not spans:
            return dex.DrawResult.Complete(region=None)

        # Centred, and small enough that the labels outside the ring still
        # fit. Measured rather than guessed from the font size: a name sits
        # entirely outside `radius + LABEL_GAP`, so the longest one is exactly
        # what has to be reserved, and guessing short clips it off the edge.
        radius = min(width, height) / 2.0 - LABEL_GAP - self._label_reach(ctx)
        if radius <= 0.0:
            return dex.DrawResult.Complete(region=None)
        cx = base.pos.x + width / 2.0
        cy = base.pos.y + height / 2.0

        # Back to front: ribbons, then the track over them, then the ring, then
        # the names. Nothing here overlaps except by design, so the order is
        # about what reads as foreground rather than about correctness.
        self._ribbons(ctx, cx, cy, radius, spans)
        self._track(ctx, cx, cy, radius, spans)
        self._ring(ctx, cx, cy, radius, spans)
        self._labels(ctx, cx, cy, radius, spans)

        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(
                base.pos, dex.Vector.new(width, height)
            )
        )

    def _label_reach(self, ctx):
        """How far past the ring the longest name reaches."""
        font = dex.Font.proportional(LABEL_FONT)
        wrap = dex.TextWrap.singleline()
        reach = 0.0
        for (name, _size, _rgb) in self.segments:
            metrics = ctx.measure_text(name, font, wrap)
            # A name on the left or right of the circle reaches by its width;
            # one at the top or bottom by its height. The radius is a single
            # number, so it has to clear the worse of the two.
            reach = max(reach, metrics.width, metrics.height)
        return reach

    def _polygon(self, ctx, points, rgb, alpha=255, stroke=None):
        """Draw one polygon in absolute coordinates.

        `Path` positions its anchors relative to the constraints it is drawn
        under, so the points go in as-is and the origin stays at zero.
        """
        if len(points) < 3:
            return
        fill = dex.Color.rgba(rgb[0], rgb[1], rgb[2], alpha)
        ctx.draw_node(
            dex.Path.polygon(
                [dex.Vector.new(x, y) for (x, y) in points],
                fill,
                stroke if stroke is not None else dex.Stroke.none(),
            ),
            dex.DrawConstraints(
                pos=dex.ScreenPos.new(0.0, 0.0),
                x=None,
                y=None,
                wrap=None,  # NotAllowed
                should_clip=False,
            ),
        )

    def _ring(self, ctx, cx, cy, radius, spans):
        """The ideogram: one filled sector per segment, with its ticks."""
        for (name, _size, rgb) in self.segments:
            a0, a1, size = spans[name]
            for part in sectors(
                cx, cy, radius * R_RING_IN, radius * R_RING_OUT, a0, a1
            ):
                self._polygon(ctx, part, rgb)
            # Ticks read inward from the ring, at a round interval.
            offset = TICK_EVERY
            while offset < size:
                angle = at(spans, name, offset)
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
                    alpha=150,
                )
                offset += TICK_EVERY

    def _track(self, ctx, cx, cy, radius, spans):
        """A bar per bin, growing inward from the outside of the track."""
        r_out = radius * R_TRACK_OUT
        r_in = radius * R_TRACK_IN
        for (name, _size, rgb) in self.segments:
            values = self.track.get(name) or []
            if not values:
                continue
            a0, a1, _size = spans[name]
            step = (a1 - a0) / len(values)
            # The well the bars sit in, so a short bar still reads as a value.
            for part in sectors(cx, cy, r_in, r_out, a0, a1):
                self._polygon(ctx, part, TRACK_BG)
            for i, value in enumerate(values):
                depth = (r_out - r_in) * max(0.0, min(value, 1.0))
                if depth <= 0.0:
                    continue
                # A hair of angle off each end, so neighbouring bars separate.
                b0 = a0 + step * i + step * 0.08
                b1 = a0 + step * (i + 1) - step * 0.08
                self._polygon(
                    ctx, sector(cx, cy, r_out - depth, r_out, b0, b1), TRACK_INK
                )

    def _ribbons(self, ctx, cx, cy, radius, spans):
        """One translucent chord per link, coloured by where it starts."""
        colours = {name: rgb for (name, _size, rgb) in self.segments}
        for link in self.links:
            (src, s0, s1, dst, d0, d1) = link
            if src not in spans or dst not in spans:
                continue
            self._polygon(
                ctx,
                ribbon(
                    cx,
                    cy,
                    radius * R_RIBBON,
                    at(spans, src, s0),
                    at(spans, src, s1),
                    at(spans, dst, d0),
                    at(spans, dst, d1),
                ),
                colours.get(src, INK),
                # Translucent, so crossing ribbons read as crossing rather
                # than as whichever happened to be drawn last.
                alpha=110,
            )

    def _labels(self, ctx, cx, cy, radius, spans):
        """Each segment's name, centred on it and clear of the ring.

        Measured rather than estimated: the label is laid out without being
        drawn, and the box it reports is what centres it. A guess from an
        average character width leaves every name a little off its segment.
        """
        font = dex.Font.proportional(LABEL_FONT)
        wrap = dex.TextWrap.singleline()
        for (name, _size, _rgb) in self.segments:
            a0, a1, _size = spans[name]
            middle = (a0 + a1) / 2.0
            metrics = ctx.measure_text(name, font, wrap)
            (x, y) = polar(cx, cy, radius + LABEL_GAP, middle)

            # Pushed out along its own radius by half its size, so the label
            # clears the ring on whichever side of the circle it is on.
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
        return "A Circos Plot"


def transform():
    """A lambda returning the plot of the hardcoded data."""
    return CircosPlot()
