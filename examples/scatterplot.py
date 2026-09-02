"""An infinite, scrollable scatter plot.

There is no chart here — no frame, no plot rectangle, no fixed extent. There is
a canvas, and the plot is simply what has been put on it:

  * `GraphPaper` rules the whole plane at a fixed pitch. One cell is one round
    step of data, so these are the plot's gridlines.
  * `PlotAxes` draws the x and y axes — the lines through data zero — and reads
    the visible gridlines off to caption them.
  * The points are ordinary canvas items, one circle each, placed at the canvas
    coordinate their value maps to. They are `StaticCanvasItem`s, which decline
    an inspector — so no lens appears over a dot, nothing can be wired to one,
    and dragging across the scatter pans the plot rather than grabbing at it.
  * `PlotLegend` and `PointReadout` are *foregrounds*, drawn over the items on
    the same terms. Chrome that the content is allowed to scribble over is not
    chrome, so the legend and the hover readout belong on top.

Pan anywhere and it keeps working. Both backgrounds are drawn the whole visible
area in screen coordinates and ask the surface for `CanvasViewOrigin` — the
canvas-space point at the top-left of what is visible — so a canvas-space point
`p` lands on screen at `constraints.pos + (p - origin)`. A background that uses
the origin scrolls with the surface; one that ignores it is pinned to the
viewport. `PlotAxes` does both at once: its tick captions travel along their own
axis with the grid, but sit at a fixed distance from the viewport edge, so the
ruler is still there after you have scrolled the axis itself out of sight. The
legend is the far end of that scale — it never asks the surface anything, and so
is simply pinned to a corner.

The readout is one sensor for the whole plot, not one per dot: `PointReadout`
draws a hover-only `InteractionBox` across the viewport, asks it where the
pointer is, and searches the data for the nearest point. Forty-two sensors would
have been the obvious way and the wrong one.

The plot carries its own data: `sample_series()` generates it from a seeded RNG,
so nothing needs wiring in. Data is stored as plain floats rather than live
`dex` values, which keeps the nodes picklable — the same rule the other Python
examples follow.
"""

import math
import random

# -- the plane ------------------------------------------------------------

# Canvas points per grid cell. One cell spans one `data_step` of data, so the
# scale of the whole plot is this over that.
CELL = 64.0
# Where the plot's own origin sits on the plane, in whole cells. Whole, so the
# gridlines still fall on round data values; and away from the plane's origin
# because canvas y grows *down* — with the two coincident, every positive value
# would be plotted above the top of the surface, out of sight.

POINT_RADIUS = 5.0
TICK_FONT = 10.0
LEGEND_FONT = 11.0
READOUT_FONT = 11.0
# How far the legend sits from the top-right corner of the viewport.
LEGEND_INSET = 10.0
LEGEND_ROW = 17.0
LEGEND_WIDTH = 108.0
# How close the pointer must come to a point, in canvas units, to read it.
HOVER_REACH = 14.0
# How far the captions sit from the left and bottom edges of the viewport.
RULER_INSET = 6.0
# Roughly how wide a digit is at `TICK_FONT`. Text cannot be measured from
# Python — a `Label` reports its region only once it has painted — so a caption
# is aligned against this estimate rather than against its real width.
DIGIT_W = 0.55

# How many cells to aim for across the data; the step is rounded to 1, 2 or 5
# times a power of ten, so the actual count lands near this.
TARGET_CELLS = 6

# Stored as plain RGB triples rather than `dex.Color` values: the palette is
# just data, and building the colour at draw time keeps these nodes picklable.
PAPER = (228, 231, 238)
AXIS = (128, 134, 146)
TEXT = (86, 92, 104)
PANEL = (255, 255, 255)
PANEL_EDGE = (206, 211, 220)
SERIES_PALETTE = [
    (72, 130, 220),   # blue
    (226, 110, 92),   # coral
    (96, 176, 118),   # green
]


# ======================================================================
# Scale — pure, and the part worth testing
# ======================================================================


def nice_step(span, target=TARGET_CELLS):
    """A round step (1, 2 or 5 times a power of ten) cutting `span` into
    roughly `target` intervals."""
    if span <= 0 or target <= 0:
        return 1.0
    raw = span / target
    magnitude = 10.0 ** math.floor(math.log10(raw))
    for factor in (1.0, 2.0, 5.0):
        if raw <= factor * magnitude:
            return factor * magnitude
    return 10.0 * magnitude


def data_step(series, target=TARGET_CELLS):
    """The data value one grid cell spans, for a plot of `series`.

    One step for both axes: the grid is square, so a unit of x has to measure
    the same as a unit of y or the shape of the scatter is a lie.
    """
    points = [point for (_name, _rgb, points) in series for point in points]
    if not points:
        return 1.0
    span = max(
        max(x for (x, _y) in points) - min(x for (x, _y) in points),
        max(y for (_x, y) in points) - min(y for (_x, y) in points),
    )
    return nice_step(span, target)


ORIGIN_CELL_X = 1
ORIGIN_CELL_Y = 6


def plot_origin():
    """The canvas-space point data `(0, 0)` is drawn at."""
    return (ORIGIN_CELL_X * CELL, ORIGIN_CELL_Y * CELL)


def to_canvas(point, step):
    """A data point as a canvas-space `(x, y)`.

    y flips on the way: data grows up, the canvas grows down.
    """
    (ox, oy) = plot_origin()
    scale = CELL / step
    return (ox + point[0] * scale, oy - point[1] * scale)


def x_value(cx, step):
    """The value a canvas-space x stands for."""
    return (cx - plot_origin()[0]) / CELL * step


def y_value(cy, step):
    """The value a canvas-space y stands for."""
    return (plot_origin()[1] - cy) / CELL * step


def cell_lines(low, high):
    """The canvas coordinates of the gridlines between `low` and `high`."""
    if CELL <= 0.0 or high < low:
        return []
    first = math.ceil(low / CELL)
    last = math.floor(high / CELL)
    return [i * CELL for i in range(first, last + 1)]


def readout_text(point, step):
    """A point as `(x, y)`, one decimal finer than the grid can show."""
    places = max(0, -math.floor(math.log10(step))) + 1 if step > 0 else 1
    return f"({point[0]:.{places}f}, {point[1]:.{places}f})"


def tick_text(value, step):
    """A tick caption with only the decimals `step` actually needs."""
    places = max(0, -math.floor(math.log10(step))) if step > 0 else 0
    text = f"{value:.{places}f}"
    return "0" if text in ("-0", "-0.0", "-0.00") else text


# ======================================================================
# The data the plot carries
# ======================================================================


def _cluster(rng, n, cx, cy, spread):
    return [(rng.gauss(cx, spread), rng.gauss(cy, spread)) for _ in range(n)]


def sample_series(seed=7):
    """Two clusters and a trend, as `(name, rgb, points)` triples."""
    rng = random.Random(seed)
    trend = [
        (x + rng.gauss(0.0, 0.4), 0.55 * x + 1.5 + rng.gauss(0.0, 0.9))
        for x in [i * 1.2 for i in range(16)]
    ]
    return [
        ("cluster A", SERIES_PALETTE[0], _cluster(rng, 16, 4.5, 12.0, 1.6)),
        ("cluster B", SERIES_PALETTE[1], _cluster(rng, 16, 12.5, 5.0, 2.0)),
        ("trend", SERIES_PALETTE[2], trend),
    ]


# ======================================================================
# Backgrounds
# ======================================================================


class _Chrome:
    """What the layers below share: the mapping onto the plane.

    None of them owns the canvas it points at — a script node owns only what
    its `owned_nodes` declares — so the back-reference is a reference, rewritten
    to the copy when the surface is deep-cloned.
    """

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

    def box(self, base, sx, sy):
        """Constraints for a box at `(sx, sy)` from the viewport's top-left.

        Screen offsets, not canvas ones: a caller that wants to scroll with the
        plane subtracts the view origin, and a caller that wants to stay put
        does not. That choice is the whole of the background API.
        """
        return dex.DrawConstraints(
            pos=dex.ScreenPos.new(base.pos.x + sx, base.pos.y + sy),
            x=None,
            y=None,
            wrap=None,  # NotAllowed
            should_clip=False,
        )

    def line(self, ctx, base, sx, sy, dx, dy, rgb, thickness):
        """A straight segment from viewport-relative `(sx, sy)`."""
        ctx.draw_node(
            dex.Path.span(
                dex.Vector.new(dx, dy),
                dex.Stroke.new(thickness, dex.Color.rgb(*rgb)),
            ),
            self.box(base, sx, sy),
        )

    def text(self, ctx, base, sx, sy, value, rgb, size=TICK_FONT):
        label = dex.Label.new(value)
        label.font = dex.Font.proportional(size)
        label.color = dex.Color.rgb(*rgb)
        ctx.draw_node(label, self.box(base, sx, sy))


class GraphPaper(_Chrome):
    """The grid, repeating across the whole plane.

    Only what is on screen is drawn — the visible edges are rounded out to the
    next cell — but that is still one `Path` node per line per frame, which is
    the cost to watch as the viewport grows.
    """

    def __init__(self, canvas, rgb=PAPER):
        super().__init__(canvas)
        self.rgb = rgb

    def draw(self, ctx):
        view = self.view(ctx)
        if view is None:
            return dex.DrawResult.Complete(region=None)
        (origin, base, width, height) = view

        for x in cell_lines(origin.x, origin.x + width):
            self.line(ctx, base, x - origin.x, 0.0, 0.0, height, self.rgb, 1.0)
        for y in cell_lines(origin.y, origin.y + height):
            self.line(ctx, base, 0.0, y - origin.y, width, 0.0, self.rgb, 1.0)

        return dex.DrawResult.Complete(region=None)

    def type_name(self):
        return "Graph Paper"


class PlotAxes(_Chrome):
    """The x and y axes, and a caption for every visible gridline.

    The axes are the lines through data zero, so they scroll away like anything
    else on the plane. The captions do not: each travels along its own axis with
    the gridline it names, but sits a fixed distance from the viewport edge, so
    what is on screen always says what it is.
    """

    def __init__(self, canvas, step):
        super().__init__(canvas)
        # Canvas points per cell is fixed; this is what a cell is worth.
        self.step = step

    def draw(self, ctx):
        view = self.view(ctx)
        if view is None:
            return dex.DrawResult.Complete(region=None)
        (origin, base, width, height) = view

        # The axes: the lines through the plot's origin, wherever those have
        # got to.
        (ox, oy) = plot_origin()
        self.line(ctx, base, 0.0, oy - origin.y, width, 0.0, AXIS, 1.5)
        self.line(ctx, base, ox - origin.x, 0.0, 0.0, height, AXIS, 1.5)

        # Captions along the bottom, one per vertical gridline.
        caption_y = height - TICK_FONT - RULER_INSET
        for x in cell_lines(origin.x, origin.x + width):
            caption = tick_text(x_value(x, self.step), self.step)
            self.text(
                ctx,
                base,
                x - origin.x - len(caption) * TICK_FONT * DIGIT_W / 2.0,
                caption_y,
                caption,
                TEXT,
            )

        # And down the left, one per horizontal gridline.
        for y in cell_lines(origin.y, origin.y + height):
            caption = tick_text(y_value(y, self.step), self.step)
            self.text(
                ctx,
                base,
                RULER_INSET,
                y - origin.y - TICK_FONT * 0.7,
                caption,
                TEXT,
            )

        return dex.DrawResult.Complete(region=None)

    def type_name(self):
        return "Plot Axes"


class PlotLegend(_Chrome):
    """A swatch and a name per series, pinned to the top-right corner.

    The far end of the API from `GraphPaper`: it never asks the surface where
    the view is, so nothing it draws moves. It is a foreground because a legend
    the content is allowed to scribble over is not a legend.
    """

    def __init__(self, names):
        # Nothing to point at: a fixed layer needs no canvas.
        super().__init__(None)
        self.names = list(names)

    def draw(self, ctx):
        base = ctx.constraints
        if not self.names or base.x is None:
            return dex.DrawResult.Complete(region=None)
        width = base.x.provided_value()

        panel_h = LEGEND_ROW * len(self.names) + 8.0
        left = width - LEGEND_INSET - LEGEND_WIDTH
        ctx.draw_node(
            dex.Rect.bordered(
                LEGEND_WIDTH,
                panel_h,
                dex.Color.rgba(PANEL[0], PANEL[1], PANEL[2], 235),
                4.0,
                dex.Stroke.new(1.0, dex.Color.rgb(*PANEL_EDGE)),
            ),
            self.box(base, left, LEGEND_INSET),
        )

        for i, name in enumerate(self.names):
            rgb = SERIES_PALETTE[i % len(SERIES_PALETTE)]
            row_y = LEGEND_INSET + 4.0 + i * LEGEND_ROW
            ctx.draw_node(
                dex.Circle.new(POINT_RADIUS, dex.Color.rgb(*rgb)),
                self.box(base, left + 8.0, row_y + 2.0),
            )
            self.text(
                ctx,
                base,
                left + 10.0 + 2.0 * POINT_RADIUS,
                row_y,
                name,
                TEXT,
                LEGEND_FONT,
            )

        return dex.DrawResult.Complete(region=None)

    def type_name(self):
        return "Plot Legend"


class PointReadout(_Chrome):
    """The value of whatever point the pointer is on.

    One sensor for the whole plot, not one per dot: a hover-only
    `InteractionBox` is drawn across the viewport, asked where the pointer is,
    and the nearest point is found by searching the data. Forty-two sensors
    would have been the obvious way and the wrong one.

    Being a foreground, it draws over the scatter — a readout under the dots
    would be unreadable exactly where it matters.
    """

    def __init__(self, canvas, sensor, series, step):
        super().__init__(canvas)
        self.sensor = sensor
        self.series = series
        self.step = step

    def owned_nodes(self):
        """The sensor is ours, and goes when we go."""
        return [self.sensor]

    def nearest(self, cx, cy):
        """`(name, rgb, point, canvas_x, canvas_y)` nearest to a canvas-space
        `(cx, cy)`, or `None` if nothing is within reach."""
        best = None
        for (name, rgb, points) in self.series:
            for point in points:
                (px, py) = to_canvas(point, self.step)
                gap = (px - cx) ** 2 + (py - cy) ** 2
                if gap <= HOVER_REACH**2 and (best is None or gap < best[0]):
                    best = (gap, name, rgb, point, px, py)
        return None if best is None else best[1:]

    def draw(self, ctx):
        view = self.view(ctx)
        if view is None:
            return dex.DrawResult.Complete(region=None)
        (origin, base, width, height) = view

        # The sensor covers the surface, and senses hover only: a click or a
        # drag caught up here would be a pan the canvas never saw.
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
        (name, rgb, point, px, py) = found

        # A ring around the point, then the caption beside it.
        (sx, sy) = (px - origin.x, py - origin.y)
        ring = POINT_RADIUS + 4.0
        ctx.draw_node(
            dex.Circle.bordered(
                ring, dex.Color.transparent(), dex.Stroke.new(1.5, dex.Color.rgb(*rgb))
            ),
            self.box(base, sx - ring, sy - ring),
        )

        caption = f"{name}  {readout_text(point, self.step)}"
        caption_w = len(caption) * READOUT_FONT * DIGIT_W + 12.0
        # Flip to the other side rather than run off the edge.
        left = sx + ring + 4.0
        if left + caption_w > width:
            left = sx - ring - 4.0 - caption_w
        top = sy - READOUT_FONT - 4.0
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
        self.text(ctx, base, left + 6.0, top + 4.0, caption, TEXT, READOUT_FONT)

        return dex.DrawResult.Complete(region=None)

    def type_name(self):
        return "Point Readout"


# ======================================================================
# Building the plot
# ======================================================================


def build(ws, series=None):
    """A canvas holding the plot; returns its uid.

    Every id is minted here rather than read back: the action queue does not
    drain until this returns, so nothing built along the way can be looked up
    before then.
    """
    series = list(sample_series() if series is None else series)
    step = data_step(series)

    canvas = dex.Canvas.build(ws)

    # The chrome, in the order it is drawn: paper and axes under the points,
    # legend and readout over them. Which layer each goes in is said here, at
    # insertion — the nodes themselves do not care.
    sensor = ws.insert_node_dyn(dex.InteractionBox.sensing(True, False, False))
    chrome = [
        (GraphPaper(canvas), dex.Layer.background()),
        (PlotAxes(canvas, step), dex.Layer.background()),
        (PlotLegend([name for (name, _rgb, _pts) in series]), dex.Layer.foreground()),
        (PointReadout(canvas, sensor, series, step), dex.Layer.foreground()),
    ]
    for (node, layer) in chrome:
        uid = dex.NodeUid.mint()
        ws.insert_node_at_dyn(uid, node)
        ws.submit_action(canvas, dex.AdoptCanvasNode(uid, layer), "Added chrome")

    # One item per point, at the canvas coordinate its value maps to — the same
    # mapping the gridlines are captioned by, so a dot sits where it reads.
    size = dex.Vector.new(2.0 * POINT_RADIUS, 2.0 * POINT_RADIUS)
    placements = []
    for (_name, rgb, points) in series:
        for point in points:
            (cx, cy) = to_canvas(point, step)
            child = dex.NodeUid.mint()
            ws.insert_node_at_dyn(
                child, dex.Circle.new(POINT_RADIUS, dex.Color.rgb(*rgb))
            )
            # Static, so the dot is content: no handle, no grips, and nothing
            # behind a lens — so no lens, and a drag across the scatter pans
            # the plot instead of grabbing at it.
            item = dex.StaticCanvasItem.build(
                ws,
                child,
                dex.Vector.new(cx - POINT_RADIUS, cy - POINT_RADIUS),
                size,
            )
            placements.append(
                (canvas, dex.AdoptCanvasNode(item, dex.Layer.midground()))
            )

    # One undo step for the whole scatter, rather than one per dot.
    ws.batch(placements, "Placed the points")
    return canvas


def transform():
    """A lambda returning a canvas holding the plot."""
    return build(dex.ws)
