"""The workspace prelude: one protocol, and a library of views that speak it.

This runs before every lambda, so everything named here is in scope wherever a
transform is written. It is also read on its own when the sidebar asks what the
prelude offers, and that is the only time a factory registered at the bottom is
called.

WHAT IS HERE

  * **The protocol.** A dozen messages every visualization answers, so a view
    can be asked where it drew a record, what record that was, and what built
    it — without knowing what kind of view it is.
  * **A `Plot` spine.** Reading a table, picking a mark, the hover readout, the
    click overlay, the whole protocol. A layout inherits it and implements one
    method: `paint`.
  * **The layouts.** Scatter, bars, strip, violin, heatmap, phylogeny
    (hierarchical and circular), circos, and a 3D projection. Each is a node in
    its own right; build one, put it on a plane, wire it to anything.
  * **The data explorer**, offered in the sidebar as a lambda, "Generate data
    explorer", that you wire a table into. A bar of dropdowns over whichever
    layout the data calls for, drawn on a plane you pan and zoom.

The join between two views — a line drawn between the same record in each — is
the library's, not a view's: `link_views` hangs a `Correspondence` over a pair,
reaching through a plane to the view on it so two explorers or trees connect as
readily as two bare plots. `examples/connect_views.py` is the worked example.

WHY THE PROTOCOL

Two plots built from the same table are two nodes that share nothing but the
rows behind them. To draw a line between the same record in a scatter and in a
tree, one view has to be able to ask the other *where did you draw row 7 this
frame, and what is in it* — without knowing what the other is. That is what
these messages are, and it is why every layout here answers them identically.

A message is matched by its `name` tag, never by class identity. A transform
defines its classes afresh each run, and saving or cloning a workspace gives
them new identities again; an `isinstance` check across those boundaries would
silently miss. A short string does not.

COORDINATES

`DrawnPoints` answers in **screen** pixels for the frame in which it is asked —
`ctx.to_global`, not the node's own drawing space. A view on a pan/zoom plane
paints on a layer carrying that plane's transform, so its own coordinates mean
nothing to anybody else; mapping out is what lets two views on two different
planes talk about the same point. A questioner must be drawn *after* the view it
asks, so the frame has been recorded before the question is put.

ROWS

`row_id` is the row's index in the source table, so the same id means the same
record in every view built from that table. Two views built from *different*
tables join on a key column instead: whichever column the view names as `"key"`
in its `Encoding`, matched by value.

WHAT IT COSTS

Nothing is precomputed per data point. There is no node per mark — a table of
any size would grind to a halt with one inspectable node redrawn and hit-tested
every frame. Each layout draws its own marks, records where they landed, and
answers hover and click by searching that record for the nearest one.

`pyarrow` is required: it is how Arrow data crosses into a script, and the row
overlay is a real `Table` node built from a one-row slice of the source.
"""

import math
import random

from typing import TypeGuard


# ======================================================================
# The protocol
# ======================================================================
#
# The tags are the whole contract between a view and whatever is asking it.
# Prefixed so nothing else collides with them, and kept in one place so a view
# that only *answers* need know nothing but these strings.

ROW_KEYS = "dex.plot.row_keys"
DRAWN_POINTS = "dex.plot.drawn_points"
DRAWN_POINT = "dex.plot.drawn_point"
ROW_VALUES = "dex.plot.row_values"
SOURCE_TABLE = "dex.plot.source_table"
ENCODING = "dex.plot.encoding"
POINT_LABEL = "dex.plot.point_label"
SELECTION = "dex.plot.selection"
HOVER_ROW = "dex.plot.hover_row"
ROW_TABLE = "dex.plot.row_table"
ROW_GROUP = "dex.plot.row_group"
PLOT_SCALE = "dex.plot.scale"
PLOT_NAME = "dex.plot.name"
KEPT_NODE = "dex.plot.kept_node"
KEEP_NODE = "dex.plot.keep_node"
PLOT_STATS = "dex.plot.stats"
SET_CHROME = "dex.plot.set_chrome"
SET_SELECTION = "dex.plot.set_selection"
SET_ENCODING = "dex.plot.set_encoding"

#: Every tag a view answers, for a surface passing questions down to its plot.
PROTOCOL = (
    ROW_KEYS, DRAWN_POINTS, DRAWN_POINT, ROW_VALUES, SOURCE_TABLE, ENCODING,
    POINT_LABEL, SELECTION, HOVER_ROW, ROW_TABLE, ROW_GROUP, PLOT_SCALE,
    PLOT_NAME, KEPT_NODE, KEEP_NODE, PLOT_STATS, SET_CHROME,
    SET_SELECTION, SET_ENCODING,
)


class RowKeys(dex.Request):
    """Every row id this view can place, in the order it draws them."""

    name = ROW_KEYS


class DrawnPoints(dex.Request):
    """Every row drawn this frame, as `{row_id: (screen_x, screen_y)}`.

    Screen coordinates, mapped out of whatever plane the view sits on. Empty
    until it has drawn once, and only the rows actually on screen this frame — a
    view showing one column at a time answers for the rows in view, not for the
    whole table.
    """

    name = DRAWN_POINTS


class DrawnPoint(dex.Request):
    """Where one row is drawn this frame, as `(screen_x, screen_y)`, or `None`."""

    name = DRAWN_POINT

    def __init__(self, row_id):
        self.row_id = row_id


class RowValues(dex.Request):
    """The record behind a row id, as a `{column: value}` dict, or `None`."""

    name = ROW_VALUES

    def __init__(self, row_id):
        self.row_id = row_id


class SourceTable(dex.Request):
    """The Arrow table this view was built from.

    What makes a *join* possible rather than only a comparison: two views can be
    asked for the data behind them and the two put together. Answered as a
    pyarrow table, the same object a wired `Table` argument arrives as.
    """

    name = SOURCE_TABLE


class Encoding(dex.Request):
    """Which column is doing what, as a dict.

    `{"kind": "scatter", "x": "body_mass_g", "y": "flipper_mm", "color": None,
      "key": None}` — `kind` names the layout, the rest name columns. `key` is
    the column identifying a record *across tables*; `None` means rows join by
    position, which is right when both views came from the same table.
    """

    name = ENCODING


class PointLabel(dex.Request):
    """One row, as the line a readout shows: `"body_mass_g: 3750, flipper_mm: 181"`.

    The view knows what it plotted, so it is the one that can say what a mark
    means. A second view showing a link can put the same words on it without
    knowing anything about the first.
    """

    name = POINT_LABEL

    def __init__(self, row_id):
        self.row_id = row_id


class Selection(dex.Request):
    """The row a click selected, or `None`."""

    name = SELECTION


class HoverRow(dex.Request):
    """The row the pointer is over this frame, or `None`."""

    name = HOVER_ROW


class RowTable(dex.Request):
    """A one-row `Table` node showing a record, as its node id — or `None`.

    The view owns it: it holds the source, so it is the one that can slice a row
    out with its real column types intact. Anything drawing a record — the
    readout pinned to a plane, a panel somewhere else entirely — asks for the
    node and draws it, rather than building a second copy to keep in step.
    """

    name = ROW_TABLE

    def __init__(self, row_id):
        self.row_id = row_id


class RowGroup(dex.Request):
    """Every row the mark holding `row_id` stands for, as a list of row ids.

    One id back, from a view with a mark per record. A whole category, from one
    whose mark *is* a category — a bar, a heatmap cell, a circos sector. What
    lets a readout drawn somewhere else say how much a click actually picked up
    without knowing which kind of picture it picked it up from.
    """

    name = ROW_GROUP

    def __init__(self, row_id):
        self.row_id = row_id


class PlotScale(dex.Request):
    """How this view maps data onto the plane, or `None` if it does not.

    `{"x": axis, "y": axis}`, each either a continuous axis — its range, its
    step, and the two positions the range spans — or a categorical one: the
    labels, and the band they are spread across. Positions are in the view's own
    drawing coordinates, which on a plane are the plane's.

    What lets the axes be drawn by something *else*: a background that knows the
    mapping can work out which values are on screen and put ticks there, rather
    than the view baking them in at one fixed size. A layout with no cartesian
    mapping — a tree, a circos — answers `None`, and the background draws
    nothing.
    """

    name = PLOT_SCALE


class PlotName(dex.Request):
    """What this view is showing, as a title.

    Asked rather than composed by whoever is drawing it, so a layout that has
    something better to say than "scatter of a against b" can say it.
    """

    name = PLOT_NAME


class KeptNode(dex.Request):
    """A node this view is keeping under `key`, or `None`.

    Views accumulate things that were expensive to get — a genome fetched for
    one tip, a structure folded for one gene — and those belong to the view
    rather than to whatever chrome happened to ask for them. A readout is
    redrawn, replaced, and rebuilt; the view outlives it, is what a clone
    copies, and is what a delete cleans up after.
    """

    name = KEPT_NODE

    def __init__(self, key):
        self.key = key


class KeepNode(dex.Request):
    """Give a view something to keep under `key`. Answers what it now holds.

    The first one wins, so two askers racing for the same key end up sharing the
    one answer rather than each keeping a copy of it.
    """

    name = KEEP_NODE

    def __init__(self, key, node):
        self.key = key
        self.node = node


class PlotStats(dex.Request):
    """The numbers a view worked out about what it drew, as lines of text.

    `["n = 90", "r = +0.874", "slope = 0.153"]`. Answered rather than drawn,
    because on a plane they belong in front of the picture: a panel of figures
    is a reading of the data, not part of it, and one drawn into the plane
    shrinks away the moment anybody zooms out.
    """

    name = PLOT_STATS


class SetChrome(dex.Request):
    """Say whether a view draws its own decoration. Answers what it settled on.

    Off when something else is doing it — the axes, title and figures around a
    view on a plane are nodes of their own, so that they hold their size while
    the picture behind them moves. On when the view is drawn straight into a box
    and there is nowhere else for them to go.
    """

    name = SET_CHROME

    def __init__(self, on):
        self.on = on


class SetSelection(dex.Request):
    """Select a row (or `None` to clear it). Answers the row it settled on.

    A request rather than an action, deliberately, for two reasons. A selection
    is view state, not an edit: nobody wants undo to walk back through every
    point they have clicked. And an action is applied to a *deep copy* of the
    node it reaches, which for a view holding a table would mean copying the
    whole table on every linked-hover update.

    Which is what this is for: hover a point in one view and push the same row
    into another, so both light up together.
    """

    name = SET_SELECTION

    def __init__(self, row_id):
        self.row_id = row_id


class SetEncoding(dex.Request):
    """Re-point a view: at other columns, or at another kind of picture.

    `SetEncoding(kind="violin", x="species", y="mass")`. Setting `kind` makes
    the view *become* that layout in place — the same node, so nothing pointing
    at it has to be rebuilt. Anything the view does not recognise is left alone.
    View state, and a request, for the same reasons as `SetSelection`.
    """

    name = SET_ENCODING

    def __init__(self, **channels):
        self.channels = channels


# ======================================================================
# Painting
# ======================================================================
#
# Free functions, not methods: every layout here paints, and the ink belongs to
# none of them in particular.


def workspace_of(ctx):
    """The workspace, whichever kind of context this is.

    A draw hands a `DrawContext`, whose workspace is one hop further in; a
    `request` or a `tick` hands a `NodeContext`, which is the hop. Code reached
    from both — the row table is asked for during a draw *and* over the
    protocol — should not have to care which it got.
    """
    node = getattr(ctx, "node", None)
    return node.workspace if node is not None else ctx.workspace


def unplaced():
    """Constraints for a shape that carries its own coordinates.

    A path or a polygon holds absolute points already, so it is drawn from the
    origin and placed by its own geometry.
    """
    return dex.DrawConstraints(
        pos=dex.ScreenPos.new(0.0, 0.0), x=None, y=None, wrap=None, should_clip=False
    )


def at(x, y, w=None, h=None):
    """Constraints placing something at `(x, y)`, sized if a size is given.

    Both axes bounded is what makes a box a node's geometry — and, for a child
    drawn with `draw_inspectable_node`, its probe region too.
    """
    return dex.DrawConstraints(
        pos=dex.ScreenPos.new(x, y),
        x=dex.AxisConstraint.Exactly(w) if w is not None else None,
        y=dex.AxisConstraint.Exactly(h) if h is not None else None,
        wrap=None, should_clip=False,
    )


def box_at(x, y, w, h):
    """`at`, but clipping whatever is drawn to the box it is given.

    For a node with a mind of its own about how big it is — a `Table` asked for
    less room than its columns want — so it scrolls inside the box rather than
    spilling out over whatever the box was drawn on top of.
    """
    return dex.DrawConstraints(
        pos=dex.ScreenPos.new(x, y),
        x=dex.AxisConstraint.Exactly(w),
        y=dex.AxisConstraint.Exactly(h),
        wrap=None, should_clip=True,
    )


def line(ctx, pts, rgb, w=1.0):
    ctx.draw_node(
        dex.Path.polyline([dex.Vector.new(x, y) for (x, y) in pts],
                          dex.Stroke.new(w, dex.Color.rgb(*rgb))),
        unplaced(),
    )


def polygon(ctx, pts, rgb, stroke=None):
    ctx.draw_node(
        dex.Path.polygon([dex.Vector.new(x, y) for (x, y) in pts],
                         dex.Color.rgb(*rgb), stroke or dex.Stroke.none()),
        unplaced(),
    )


def dot(ctx, cx, cy, r, rgb):
    ctx.draw_node(dex.Circle.new(r, dex.Color.rgb(*rgb)), at(cx - r, cy - r))


def ring(ctx, cx, cy, r, rgb, w=1.5):
    ctx.draw_node(
        dex.Circle.bordered(r, dex.Color.transparent(),
                            dex.Stroke.new(w, dex.Color.rgb(*rgb))),
        at(cx - r, cy - r),
    )


def text(ctx, s, x, y, size, rgb, bold=False):
    label = dex.Label.new(s)
    font = dex.Font.proportional(size)
    font.bold = bold
    label.font = font
    label.color = dex.Color.rgb(*rgb)
    ctx.draw_node(label, at(x, y))


def measure(ctx, s, size, bold=False):
    font = dex.Font.proportional(size)
    font.bold = bold
    m = ctx.measure_text(s, font, dex.TextWrap.singleline())
    return (m.width, m.height)


def rect(ctx, x, y, w, h, rgb, edge=None, radius=0.0, alpha=255):
    """A filled box, optionally bordered. `alpha` is the fill's opacity."""
    fill = dex.Color.rgba(rgb[0], rgb[1], rgb[2], alpha)
    if edge is None:
        ctx.draw_node(dex.Rect.new(w, h, fill), at(x, y, w, h))
    else:
        ctx.draw_node(
            dex.Rect.bordered(w, h, fill, radius,
                              dex.Stroke.new(1.0, dex.Color.rgb(*edge))),
            at(x, y, w, h),
        )


def card(ctx, x, y, w, h, lines, font_size, ink, edge, bold_first=True):
    """A small panel of text lines — the shape every readout in here takes."""
    rect(ctx, x, y, w, h, (255, 255, 255), edge, 4.0, 244)
    step = font_size + 4.0
    for (k, s) in enumerate(lines):
        text(ctx, s, x + 7.0, y + 5.0 + k * step, font_size, ink,
             bold=(bold_first and k == 0))


def card_size(ctx, lines, font_size):
    """How big `card` comes out for these lines."""
    width = max([measure(ctx, s, font_size)[0] for s in lines] or [0.0]) + 14.0
    return (width, len(lines) * (font_size + 4.0) + 8.0)


# ======================================================================
# Colour
# ======================================================================

INK = (44, 49, 58)
FAINT = (122, 128, 138)
AXIS = (150, 156, 166)
GRID = (232, 234, 240)
PANEL = (255, 255, 255)
PANEL_EDGE = (208, 213, 222)
POINT = (72, 130, 220)
LINE = (226, 110, 92)

#: One colour per category, cycled. Chosen to stay apart in hue *and* value, so
#: they survive being printed, dimmed, or looked at by someone colourblind.
CATEGORY_PALETTE = [
    (72, 130, 220), (226, 110, 92), (96, 176, 118), (176, 132, 210),
    (222, 168, 70), (86, 188, 196), (210, 120, 160), (140, 150, 160),
]

#: The sequential ramp a heatmap counts along.
HEAT_LOW = (240, 243, 248)
HEAT_HIGH = (36, 92, 170)


def category_color(i):
    return CATEGORY_PALETTE[i % len(CATEGORY_PALETTE)]


def lerp_rgb(a, b, t):
    t = max(0.0, min(1.0, t))
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def tint(rgb, toward=(255, 255, 255), t=0.72):
    """`rgb` washed out toward white — a fill behind something drawn in `rgb`."""
    return lerp_rgb(rgb, toward, t)


def shade(rgb, t=0.25):
    """`rgb` darkened, for the edge of a shape filled with it."""
    return lerp_rgb(rgb, (0, 0, 0), t)


def hsv_rgb(h, s, v):
    """HSV in `[0, 1]` to an RGB triple, for generating a palette of any size."""
    i = int(h * 6.0)
    f = h * 6.0 - i
    p, q, t = v * (1 - s), v * (1 - s * f), v * (1 - s * (1 - f))
    (r, g, b) = [(v, t, p), (q, v, p), (p, v, t), (p, q, v), (t, p, v), (v, p, q)][i % 6]
    return (int(r * 255), int(g * 255), int(b * 255))


def spread_palette(n):
    """`n` colours around the wheel, for a column with more levels than the
    fixed palette has entries."""
    if n <= len(CATEGORY_PALETTE):
        return [category_color(i) for i in range(n)]
    return [hsv_rgb((i / max(n, 1)) % 1.0, 0.45, 0.72) for i in range(n)]


# ======================================================================
# Statistics — pure, and the part worth testing
# ======================================================================


def finite_pairs(xs, ys):
    """The `(x, y)` where both are real numbers: the rows a numeric plot uses."""
    out = []
    for x, y in zip(xs, ys):
        if isinstance(x, bool) or isinstance(y, bool):
            continue
        if isinstance(x, (int, float)) and isinstance(y, (int, float)):
            if math.isfinite(x) and math.isfinite(y):
                out.append((float(x), float(y)))
    return out


def pearson(pairs):
    """Pearson's r for `(x, y)` pairs, or `None` if it is undefined."""
    n = len(pairs)
    if n < 2:
        return None
    mx = sum(x for x, _ in pairs) / n
    my = sum(y for _, y in pairs) / n
    sxx = sum((x - mx) ** 2 for x, _ in pairs)
    syy = sum((y - my) ** 2 for _, y in pairs)
    sxy = sum((x - mx) * (y - my) for x, y in pairs)
    if sxx <= 0.0 or syy <= 0.0:
        return None
    return sxy / math.sqrt(sxx * syy)


def least_squares(pairs):
    """`(slope, intercept)` of the line y = a·x + b, or `None`."""
    n = len(pairs)
    if n < 2:
        return None
    mx = sum(x for x, _ in pairs) / n
    my = sum(y for _, y in pairs) / n
    sxx = sum((x - mx) ** 2 for x, _ in pairs)
    if sxx <= 0.0:
        return None
    slope = sum((x - mx) * (y - my) for x, y in pairs) / sxx
    return (slope, my - slope * mx)


def quartiles(values):
    """`(q1, median, q3)` of `values`, by linear interpolation."""
    xs = sorted(values)
    if not xs:
        return (0.0, 0.0, 0.0)

    def q(p):
        if len(xs) == 1:
            return xs[0]
        pos = p * (len(xs) - 1)
        lo = int(math.floor(pos))
        hi = min(lo + 1, len(xs) - 1)
        return xs[lo] + (xs[hi] - xs[lo]) * (pos - lo)

    return (q(0.25), q(0.5), q(0.75))


def cramers_v(counts, n_rows, n_cols, total):
    """Cramér's V for a contingency table, a 0..1 strength of association."""
    if total <= 0 or n_rows < 2 or n_cols < 2:
        return None
    row_tot = [sum(counts[r][c] for c in range(n_cols)) for r in range(n_rows)]
    col_tot = [sum(counts[r][c] for r in range(n_rows)) for c in range(n_cols)]
    chi = 0.0
    for r in range(n_rows):
        for c in range(n_cols):
            expected = row_tot[r] * col_tot[c] / total
            if expected > 0.0:
                chi += (counts[r][c] - expected) ** 2 / expected
    denom = total * (min(n_rows, n_cols) - 1)
    return math.sqrt(chi / denom) if denom > 0 else None


def gaussian_kde(values, grid, bandwidth=None):
    """A Gaussian kernel density estimate of `values`, sampled at `grid`.

    Silverman's rule picks the bandwidth when none is given. This is what turns
    a strip of points into a violin: the density is the width.
    """
    n = len(values)
    if n == 0:
        return [0.0] * len(grid)
    if bandwidth is None:
        mean = sum(values) / n
        var = sum((v - mean) ** 2 for v in values) / n if n > 1 else 0.0
        sd = math.sqrt(var)
        (q1, _, q3) = quartiles(values)
        spread = min(sd, (q3 - q1) / 1.349) if q3 > q1 else sd
        bandwidth = 0.9 * (spread or sd or 1.0) * (n ** -0.2)
    if bandwidth <= 0.0:
        bandwidth = 1.0
    scale = 1.0 / (n * bandwidth * math.sqrt(2.0 * math.pi))
    out = []
    for g in grid:
        total = 0.0
        for v in values:
            z = (g - v) / bandwidth
            # Beyond four bandwidths the kernel is worth less than the
            # arithmetic; skipping it is what keeps a violin cheap on a big
            # column.
            if -4.0 < z < 4.0:
                total += math.exp(-0.5 * z * z)
        out.append(total * scale)
    return out


def nice_bounds(lo, hi, target=5):
    """A rounded `(lo, hi, step)` covering `[lo, hi]` in ~`target` steps."""
    if not math.isfinite(lo) or not math.isfinite(hi):
        return (0.0, 1.0, 0.5)
    if hi <= lo:
        pad = abs(lo) * 0.5 or 1.0
        lo, hi = lo - pad, hi + pad
    raw = (hi - lo) / max(target, 1)
    mag = 10.0 ** math.floor(math.log10(raw)) if raw > 0 else 1.0
    step = mag
    for factor in (1.0, 2.0, 5.0, 10.0):
        step = factor * mag
        if raw <= step:
            break
    return (math.floor(lo / step) * step, math.ceil(hi / step) * step, step)


def axis_ticks(lo, hi, step):
    """The tick values from `lo` to `hi` at `step`, guarding against drift."""
    if step <= 0:
        return [lo]
    out = []
    v = lo
    while v <= hi + step * 0.5 and len(out) < 200:
        out.append(round(v, 10))
        v += step
    return out


def tick_text(value, step):
    """A tick caption with only the decimals `step` needs."""
    places = max(0, -math.floor(math.log10(step))) if step > 0 else 0
    caption = "%.*f" % (places, value)
    return "0" if caption in ("-0", "-0.0", "-0.00") else caption


def is_num(v) -> TypeGuard[float]:
    """Whether `v` is a real number — `True` is not one, whatever Python says.

    Annotated as a type guard so an editor knows that what follows a successful
    check is a number. Half this file reads a column into `float(...)` behind
    one of these, and without the annotation every one of those is an error.
    """
    return isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v)


def show(v):
    """A value as it should read in a readout."""
    if isinstance(v, float):
        return "%.4g" % v
    if v is None:
        return "—"
    return str(v)


# ======================================================================
# Reading the table
# ======================================================================

#: A numeric column with no more distinct values than this is really a category.
CATEGORICAL_MAX_LEVELS = 12
#: How many categories an axis will draw before it stops.
MAX_CATEGORIES = 40


def _distinct_count(values, cap):
    """How many distinct non-null values, counting only up to `cap` + 1."""
    seen = set()
    for v in values:
        if v is None:
            continue
        seen.add(v)
        if len(seen) > cap:
            return len(seen)
    return len(seen)


def classify(values, is_numeric):
    """Whether a column is `"continuous"` or `"categorical"`.

    Text and booleans are categorical. Numbers are continuous, unless they take
    only a few distinct values — a 1/2/3 rating, a 0/1 flag — in which case they
    are a category that happens to be written as a number.
    """
    if not is_numeric:
        return "categorical"
    if _distinct_count(values, CATEGORICAL_MAX_LEVELS) <= CATEGORICAL_MAX_LEVELS:
        return "categorical"
    return "continuous"


def sample_table():
    """A small, real-shaped sample: three species across islands, with a few
    body measurements. Made up, but shaped like the penguins everyone plots."""
    rng = random.Random(7)
    species = [
        ("Adelie", "Torgersen", 3700, 190, 39.0),
        ("Adelie", "Biscoe", 3800, 189, 38.8),
        ("Gentoo", "Biscoe", 5050, 217, 47.5),
        ("Chinstrap", "Dream", 3730, 196, 48.8),
    ]
    rows = {"species": [], "island": [], "sex": [],
            "body_mass_g": [], "flipper_mm": [], "bill_length_mm": []}
    for _ in range(90):
        base = rng.choice(species)
        rows["species"].append(base[0])
        rows["island"].append(base[1])
        rows["sex"].append(rng.choice(["male", "female"]))
        rows["body_mass_g"].append(round(rng.gauss(base[2], 300)))
        rows["flipper_mm"].append(round(rng.gauss(base[3], 6)))
        rows["bill_length_mm"].append(round(rng.gauss(base[4], 2.2), 1))
    return rows


def require_pyarrow():
    """pyarrow, or a message saying what to do about not having it.

    In one place because every entry point needs it, and because the raw
    `ModuleNotFoundError` says nothing about which environment was looked in or
    how to change it — which is the only thing worth knowing when you see it.
    """
    try:
        import pyarrow
        return pyarrow
    except ImportError:
        raise ImportError(
            "the default prelude needs pyarrow: it is how Arrow data crosses "
            "into a script, and how a selected row is shown as a real table. "
            "Install it into this workspace's environment, or point dex at one "
            "that has it under Settings > Global environment. With nothing set, "
            "dex uses an activated environment, else the nearest `.venv` at or "
            "above the directory it was started in."
        )


def as_arrow(source):
    """Whatever `source` is, as a pyarrow table.

    A wired `Table` argument already arrives as one. A dict of lists is built
    into one, so a view over a hand-made column set has a real table behind it
    and can answer `SourceTable` and show a row like any other.
    """
    pa = require_pyarrow()
    if source is None:
        source = sample_table()
    if isinstance(source, dict):
        return pa.table({name: pa.array(values) for (name, values) in source.items()})
    if hasattr(source, "combine_chunks"):
        return source
    # A RecordBatch, or anything else pyarrow can take a table from.
    return pa.table(source)


class Frame:
    """The table behind a view, read once and kept.

    Holds the Arrow table itself — that is what `SourceTable` answers with and
    what the row overlay slices — alongside the plain Python lists a layout
    actually plots. Both, because reading a pyarrow column costs the same
    whether you want one value or all of them, and a layout wants all of them
    every frame.
    """

    def __init__(self, source=None):
        # The table first: `as_arrow` is where the missing-pyarrow message is,
        # and an import here would raise past it with the bare one instead.
        self.table = as_arrow(source)
        import pyarrow.types as pat

        self.columns = list(self.table.column_names)
        self.data = {}
        self.kinds = {}
        for name in self.columns:
            column = self.table.column(name)
            self.data[name] = column.to_pylist()
            numeric = (pat.is_integer(column.type) or pat.is_floating(column.type)) \
                and not pat.is_boolean(column.type)
            self.kinds[name] = classify(self.data[name], numeric)
        self.n = self.table.num_rows

    # -- asking about columns --------------------------------------------

    def kind(self, column):
        return self.kinds.get(column)

    def is_continuous(self, column):
        return self.kinds.get(column) == "continuous"

    def values(self, column):
        return self.data.get(column, [])

    def value(self, column, row):
        values = self.data.get(column)
        if values is None or not (0 <= row < len(values)):
            return None
        return values[row]

    def continuous(self):
        return [c for c in self.columns if self.is_continuous(c)]

    def categorical(self):
        return [c for c in self.columns if not self.is_continuous(c)]

    # -- asking about rows -----------------------------------------------

    def row(self, i):
        """The whole record at `i`, or `None` past the end."""
        if not (0 <= i < self.n):
            return None
        return {c: self.data[c][i] for c in self.columns}

    def row_slice(self, i):
        """Row `i` as a one-row Arrow table."""
        if not (0 <= i < self.n):
            return None
        return self.table.slice(i, 1)

    def rows_slice(self, rows):
        """Those rows as an Arrow table — what the click readout shows.

        A `take` rather than a slice, because the rows behind one mark are not
        generally next to each other: the rows a bar counts, or a heatmap cell
        crosses, are scattered the length of the table. One row still slices,
        which is the common case and costs nothing.
        """
        rows = [i for i in rows if 0 <= i < self.n]
        if not rows:
            return None
        if len(rows) == 1:
            return self.table.slice(rows[0], 1)
        return self.table.take(rows)

    def levels(self, column, cap=MAX_CATEGORIES):
        """The distinct categories of `column`, in first-seen order, capped."""
        out = []
        for v in self.values(column):
            if v is not None and v not in out:
                out.append(v)
                if len(out) >= cap:
                    break
        return out

    def tally(self, column):
        """`(levels, counts, rows_by_level)` for a categorical column."""
        levels = self.levels(column)
        rank = {v: j for (j, v) in enumerate(levels)}
        counts = [0] * len(levels)
        rows = {v: [] for v in levels}
        for (i, v) in enumerate(self.values(column)):
            if v in rank:
                counts[rank[v]] += 1
                rows[v].append(i)
        return (levels, counts, rows)

    def rows_by_key(self, key_column):
        """`{value: row_id}` for a key column — how two tables are joined."""
        out = {}
        for (i, v) in enumerate(self.values(key_column)):
            if v is not None and v not in out:
                out[v] = i
        return out


# ======================================================================
# The plot spine
# ======================================================================

#: How close the pointer must come to a mark, in pixels.
HOVER_REACH = 12.0
POINT_R = 4.0
TICK_FONT = 10.0
BAR_FONT = 11.0
READOUT_FONT = 11.0
STAT_FONT = 11.0
TITLE_FONT = 13.0

#: Room the axes take: a left gutter for values, a foot for categories, and a
#: band over the plot for its title.
GUTTER = 52.0
FOOT = 34.0
TOP = 26.0
MARGIN = 14.0

#: How the readout's `Table` lays itself out, so a card can be built the size of
#: what it is about to hold. Its header, one of its rows, and the room its
#: horizontal scroll bar takes to reach the columns that did not fit.
TABLE_HEADER_H = 24.0
TABLE_ROW_H = 21.0
TABLE_CHROME = 12.0
#: The most rows the readout is drawn tall enough for. Past this the table
#: scrolls inside the card: a bar standing for four thousand records is a table,
#: not a card, and a card as tall as the screen shows the picture behind it
#: none the better.
ROW_TABLE_MAX_ROWS = 10
#: How wide the click overlay is, before its readout asks for more. Wide enough
#: for a few columns of a record; a wider one scrolls inside its own table.
ROW_CARD_W = 460.0
#: How many points a violin's density curve is sampled at. Enough to read as a
#: curve, few enough that a wide table stays cheap.
VIOLIN_STEPS = 48


def selection_lines(row, count, label):
    """The readout's heading for a selection, and the label under it.

    The count when a mark stands for many, because the number *is* the reading —
    that is what a bar chart is showing — and the row id when it stands for one,
    because then the record is the thing itself.
    """
    head = "%d rows" % count if count != 1 else "Row %d" % row
    return [head] + [s for s in (label or "").split(", ") if s]


def row_table_height(count):
    """How tall the readout's table has to be to show `count` rows.

    Sized to what it holds rather than to a constant: a card built for one row
    and handed one leaves a row-shaped band of nothing under it, which reads as
    a second, empty record rather than as slack.
    """
    rows = max(1, min(count, ROW_TABLE_MAX_ROWS))
    return TABLE_HEADER_H + rows * TABLE_ROW_H + TABLE_CHROME


def paint_selection(ctx, x, y, w, h, lines, table, count):
    """The click readout, pinned to the bottom-right of `(x, y, w, h)`.

    A few lines saying what was clicked, and under them the records behind it as
    a real `Table` node. One drawing, used by a view drawing its own readout and
    by the chrome in a plane's foreground drawing it for one — the two differ in
    where the table comes from, and in nothing else.
    """
    (cw, ch) = card_size(ctx, lines, READOUT_FONT)
    table_h = 0.0
    if table is not None:
        # As tall as the rows it holds, but never taller than the box it is
        # pinned inside — a card running off the top of the picture is worse
        # than one whose table scrolls.
        room = max(h - ch - 24.0, TABLE_HEADER_H + TABLE_ROW_H)
        table_h = min(row_table_height(count), room)
    width = max(min(w - 16.0, max(cw, ROW_CARD_W)), 80.0)
    height = ch + (table_h + 6.0 if table is not None else 0.0)
    left = x + w - width - 8.0
    top = max(y + 4.0, y + h - height - 8.0)
    rect(ctx, left, top, width, height, PANEL, PANEL_EDGE, 5.0, 250)
    step = READOUT_FONT + 4.0
    for (k, s) in enumerate(lines):
        text(ctx, s, left + 9.0, top + 6.0 + k * step, READOUT_FONT, INK,
             bold=(k == 0))
    if table is not None:
        # Clipped: the table would rather be as wide as its columns and as tall
        # as its rows, and it is being asked to fit in a card.
        ctx.draw_node(table, box_at(left + 6.0, top + ch, width - 12.0, table_h))


class Plot:
    """What every view here is: a table, a sensor, and marks that know their row.

    A layout inherits this and implements `paint`. Everything else — picking,
    the readout, the overlay, the protocol — is here, which is the point: a
    scatter and a phylogeny answer the same questions because they answer them
    with the same code.

    **Marks, not nodes.** A layout draws its own marks and calls `record` for
    each, and hover and click are answered by searching what was recorded. One
    inspectable node per data point does not scale — it is redrawn and
    hit-tested every frame — and a table has as many points as it likes.

    **Two coordinate spaces.** `record` keeps both: the node's own, for picking
    (the pointer arrives in the same space), and the screen's, for answering
    `DrawnPoints` (a questioner may be anywhere). On a plane these differ by
    that plane's pan and zoom; off one they are the same.
    """

    #: What `Encoding` calls this layout. Subclasses override it.
    KIND = "plot"
    #: The channels a layout of this kind reads, in the order a readout lists
    #: them. Subclasses override it.
    CHANNELS = ("x",)
    #: Whether this layout wants the drag for itself. Almost none do: a plane is
    #: panned by dragging it, and a view that takes that gesture takes it from
    #: the plane. `Scatter3D` is the exception, and turns instead.
    SENSES_DRAG = False
    #: Whether this layout is worth putting on a pan/zoom plane.
    #:
    #: A picture with as many marks as the table has rows is: there is always
    #: more of it than fits, and zooming into a crowded corner is how you read
    #: it. A picture with one mark per *category* is not — it is as big as it
    #: needs to be, and a plane would only add a gesture that does nothing.
    WANTS_PLANE = False

    def __init__(self, frame, sensor, **encoding):
        self.frame = frame
        self.sensor = sensor
        self.enc = {"kind": self.KIND, "x": None, "y": None, "z": None,
                    "color": None, "key": None}
        self.enc.update({k: v for (k, v) in encoding.items() if v is not None})
        # The row a click selected, and the one the pointer is over. Both view
        # state: mutated in place, never through an action, so neither shows up
        # in the undo history. See `SetSelection`.
        self.selected = None
        self.hovered = None
        # Whether this view draws its own hover card and click overlay. A view
        # on a plane hands both to a `PlotChrome` in the plane's foreground
        # instead, so they stay legible while the plane is magnified.
        self.chrome = True
        # Rebuilt every frame by `record`.
        self._local_pos = {}
        self._screen_pos = {}
        # How this frame mapped data onto the plane; `None` for a layout with no
        # cartesian mapping. Published by `publish_axis`, read by `PlotScale`.
        self._scale = None
        # The figures this frame worked out. Published by `publish_stats`.
        self._stats = []
        # The `Table` node the overlay shows, and which rows it is for.
        self._row_table = None
        self._row_table_for = None
        # What `rows_at` last answered, and for which row. Worked out once per
        # selection: a bar's group costs a pass over the table, and the readout
        # asks for it every frame.
        self._group_for = None
        self._group = []
        # Whatever was expensive to get, by whatever key got it. See `KeepNode`.
        self._kept = {}
        # A stable sideways jitter per row, so a strip does not shimmer.
        self._jit = [random.Random(i * 2654435761).uniform(-1.0, 1.0)
                     for i in range(self.frame.n)]

    # -- persistence -----------------------------------------------------

    def __getstate__(self):
        """Everything but the frame's own positions: they belong to a screen and
        a frame, not to the view."""
        state = self.__dict__.copy()
        state["_local_pos"] = {}
        state["_screen_pos"] = {}
        state["_scale"] = None
        state["_stats"] = []
        return state

    def owned_nodes(self):
        owned = [self.sensor]
        if self._row_table is not None:
            owned.append(self._row_table)
        # What it is keeping is its own: a clone gets copies, and a delete takes
        # them with it rather than leaving them behind with nothing pointing at
        # them.
        owned.extend(self._kept.values())
        return owned

    def on_delete(self, ctx):
        for uid in self.owned_nodes():
            ctx.workspace.delete_node(uid)

    # -- what this view is -----------------------------------------------

    def channels(self):
        """The `(channel, column)` pairs this view is actually plotting."""
        out = []
        for channel in self.CHANNELS:
            column = self.enc.get(channel)
            if column:
                out.append((channel, column))
        return out

    def title(self):
        """What the view is showing, as a phrase."""
        cols = [col for (_, col) in self.channels()]
        return " x ".join(cols) if cols else "nothing"

    def type_name(self):
        return "%s of %s" % (self.KIND.title(), self.title())

    def point_label(self, row):
        """One row as a readout line: the columns this view plotted, and their
        values. What `PointLabel` answers, and what the hover card shows."""
        parts = ["%s: %s" % (col, show(self.frame.value(col, row)))
                 for (_, col) in self.channels()]
        return ", ".join(parts)

    # -- what a mark stands for ------------------------------------------

    def rows_at(self, row):
        """Every record the mark holding `row` stands for.

        One, for a layout with a mark per record — a scatter, a strip, a cloud.
        A layout whose mark is a *category* overrides this and answers the whole
        group, because that is what the click asked about: a bar is four hundred
        rows, and showing one arbitrary member of it as though it were the bar
        is the wrong answer to the question.
        """
        return [row] if self.frame.row(row) is not None else []

    def group(self, row):
        """`rows_at`, worked out once per selection rather than once a frame."""
        if row != self._group_for:
            self._group_for = row
            self._group = self.rows_at(row)
        return self._group

    def forget_group(self):
        """Drop what was worked out about the selection.

        Called when the columns change: which rows a mark stands for is a
        question about the encoding, and the old answer is about the old one.
        """
        self._group_for = None
        self._group = []
        self._row_table_for = None

    def point_inks(self, rows):
        """One colour per drawn row: by the `color` column if there is one.

        On the spine rather than on any one layout: a scatter, a cloud and
        anything else drawing a mark per record all colour them the same way,
        and a layout that does not read `color` simply never calls this.
        """
        column = self.enc.get("color")
        if not column:
            return [POINT] * len(rows)
        levels = self.frame.levels(column)
        palette = spread_palette(len(levels))
        rank = {v: j for (j, v) in enumerate(levels)}
        return [palette[rank[self.frame.value(column, i)]]
                if self.frame.value(column, i) in rank else POINT
                for i in rows]

    # -- the protocol ----------------------------------------------------

    def request(self, req, ctx):
        """Every message in the protocol, answered here so no layout has to.

        Matched by the message's `name` tag, never its class: a prelude defines
        its classes afresh on every run, and saving or cloning gives them new
        identities again, so an `isinstance` check would silently miss.
        """
        tag = getattr(req, "name", None)
        if tag == ROW_KEYS:
            return list(range(self.frame.n))
        if tag == DRAWN_POINTS:
            return dict(self._screen_pos)
        if tag == DRAWN_POINT:
            return self._screen_pos.get(req.row_id)
        if tag == ROW_VALUES:
            return self.frame.row(req.row_id)
        if tag == SOURCE_TABLE:
            return self.frame.table
        if tag == ENCODING:
            return dict(self.enc)
        if tag == POINT_LABEL:
            return self.point_label(req.row_id) if self.frame.row(req.row_id) else None
        if tag == SELECTION:
            return self.selected
        if tag == HOVER_ROW:
            return self.hovered
        if tag == ROW_TABLE:
            return self.row_table(ctx, req.row_id)
        if tag == ROW_GROUP:
            return list(self.group(req.row_id))
        if tag == PLOT_SCALE:
            return self._scale
        if tag == PLOT_NAME:
            return self.type_name()
        if tag == KEPT_NODE:
            return self._kept.get(req.key)
        if tag == KEEP_NODE:
            self._kept.setdefault(req.key, req.node)
            return self._kept[req.key]
        if tag == PLOT_STATS:
            return list(self._stats)
        if tag == SET_CHROME:
            self.chrome = bool(req.on)
            return self.chrome
        if tag == SET_SELECTION:
            row = req.row_id
            self.selected = row if (row is not None and self.frame.row(row)) else None
            return self.selected
        if tag == SET_ENCODING:
            # `kind` first: becoming another layout fills in the channels that
            # layout needs, and the named ones then win over those defaults.
            kind = req.channels.get("kind")
            if kind is not None and kind in LAYOUTS_BY_KIND:
                self.become(LAYOUTS_BY_KIND[kind])
            for (channel, column) in req.channels.items():
                if channel in CHANNEL_NAMES and (
                    column is None or column in self.frame.columns
                ):
                    self.enc[channel] = column
            self.forget_group()
            self.on_encoding_changed()
            return dict(self.enc)
        return NotImplemented

    def on_encoding_changed(self):
        """A hook for a layout that caches anything derived from its columns."""

    def become(self, layout):
        """Turn this view into another kind of picture, in place.

        The node stays the same node: same id, same sensor, same table, same
        place on whatever canvas it is sitting on. Which matters because a view
        is pointed at from several directions at once — a canvas item, a readout
        in the foreground, whatever else has wired itself to it — and rebuilding
        it as a new node would mean chasing every one of those down.

        A layout that carries configuration of its own declares it as class
        attributes as well as setting it in `__init__`, so an instance that
        becomes one finds sane values rather than missing ones.
        """
        if layout is type(self) or not issubclass(layout, Plot):
            return
        # Reassigning `__class__` is ordinary Python and exactly what is wanted
        # here, but it is not something a type checker can express: it only
        # knows that the class it is being handed is not the one this instance
        # was made with, which is the whole point.
        self.__class__ = layout  # type: ignore[assignment]
        self.enc["kind"] = layout.KIND
        # Whatever it was showing may not be a channel this layout reads, and
        # what this one needs may never have been set.
        self.enc.update(default_encoding(self.frame, layout, {
            c: self.enc.get(c) for c in CHANNEL_NAMES if self.enc.get(c)
        }))
        self.forget_group()
        self.on_encoding_changed()

    # -- recording what was drawn ----------------------------------------

    def record(self, ctx, row, cx, cy):
        """Note that row `row` was drawn at `(cx, cy)` — all the picking there is.

        Kept twice: once in this node's own coordinates, which is where the
        pointer will arrive, and once mapped out to the screen, which is the
        only space two views on two planes can agree about.
        """
        self._local_pos[row] = (cx, cy)
        point = ctx.to_global(dex.ScreenPos.new(cx, cy))
        self._screen_pos[row] = (point.x, point.y)

    def paint_title(self, ctx, x, y, w):
        """This view's name, centred over it — the same words `PlotTitle` shows
        when a plane is drawing the furniture instead."""
        caption = self.type_name()
        if not caption:
            return
        (cw, _ch) = measure(ctx, caption, TITLE_FONT, bold=True)
        text(ctx, caption, x + (w - cw) / 2.0, y + 4.0, TITLE_FONT, INK, bold=True)

    def publish_stats(self, ctx, x, y, w, lines):
        """Say what this frame worked out, and draw it if it is this view's to
        draw. See `PlotStats`."""
        self._stats = list(lines)
        if self.chrome:
            stats_card(ctx, x, y, w, lines)

    def publish_axis(self, ctx, x_axis=None, y_axis=None):
        """Say how this frame maps data onto the plane, and draw the axes if
        they are this view's to draw.

        A view on a plane hands both to the plane's background — which is the
        point of publishing rather than painting: the background can put ticks
        where the reader is looking, and they stay one size however far in the
        plane is zoomed.
        """
        scale = {}
        for (channel, axis) in (("x", x_axis), ("y", y_axis)):
            if axis is None:
                continue
            scale[channel] = axis.scale() if hasattr(axis, "scale") else axis
            if self.chrome and hasattr(axis, "paint"):
                axis.paint(ctx)
        self._scale = scale or None

    def nearest(self, pointer):
        """The row whose mark is nearest the pointer, within reach, or `None`."""
        best = None
        for (row, (cx, cy)) in self._local_pos.items():
            gap = (cx - pointer.x) ** 2 + (cy - pointer.y) ** 2
            if gap <= HOVER_REACH ** 2 and (best is None or gap < best[0]):
                best = (gap, row)
        return None if best is None else best[1]

    # -- drawing ---------------------------------------------------------

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        h = base.y.provided_value() if base.y is not None else None
        if w is None or h is None or not (math.isfinite(w) and math.isfinite(h)):
            return dex.DrawResult.Complete(region=None)
        (x, y) = (base.pos.x, base.pos.y)
        self._local_pos = {}
        self._screen_pos = {}

        if self.frame.n and w > 40.0 and h > 40.0:
            self.paint(ctx, x, y, w, h)
            # A view drawn straight into a box titles itself, because the plane
            # that would otherwise have done it is not there. Every view is
            # titled the same way either way round: what differs between a bar
            # chart and a scatter should be the picture, not the furniture.
            if self.chrome:
                self.paint_title(ctx, x, y, w)
        elif not self.frame.n:
            text(ctx, "nothing to plot", x + MARGIN, y + MARGIN, TITLE_FONT, FAINT)

        self.interact(ctx, x, y, w, h)
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))

    def paint(self, ctx, x, y, w, h):
        """Draw the marks into the box `(x, y, w, h)`, calling `record` for each.

        The one thing a layout must implement.
        """
        raise NotImplementedError("a layout must paint itself")

    def interact(self, ctx, x, y, w, h):
        """The sensor, the pick, and — unless a plane is showing them — the
        hover card and the click overlay.

        The sensor covers the view and nothing else. A sensor takes every click
        over it, so one stretched past the view would swallow the presses meant
        for whatever is around it.
        """
        ws = ctx.node.workspace
        ctx.draw_node(self.sensor, at(x, y, w, h))
        pointer = ws.send_request(self.sensor, dex.PointerPos())
        self.hovered = self.nearest(pointer) if pointer is not None else None

        if ws.send_request(self.sensor, dex.TakeClicked()):
            self.selected = self.hovered

        if not self.chrome:
            return
        if self.hovered is not None:
            self.draw_hover(ctx, x, y, w, h, self.hovered)
        if self.selected is not None:
            self.draw_overlay(ctx, x, y, w, h, self.selected)

    def draw_hover(self, ctx, x, y, w, h, row):
        """A ring on the mark, and a card beside it naming the record."""
        point = self._local_pos.get(row)
        if point is None:
            return
        (cx, cy) = point
        ring(ctx, cx, cy, POINT_R + 4.0, INK, 1.5)
        lines = ["row %d" % row] + self.point_label(row).split(", ")
        (cw, ch) = card_size(ctx, lines, READOUT_FONT)
        left = min(cx + 10.0, x + w - cw - 4.0)
        top = min(max(cy - ch - 8.0, y + 4.0), y + h - ch - 4.0)
        card(ctx, left, top, cw, ch, lines, READOUT_FONT, INK, PANEL_EDGE)

    def row_table(self, ctx, row):
        """The `Table` node for what the mark holding `row` stands for, built
        the first time it is asked for and kept until the selection moves.

        A real table, not a drawn grid: it is the same node a wired `Table` is,
        so the records show with their real column types and formatting. For a
        mark standing for many rows — a bar, a heatmap cell — that is all of
        them, which is what was clicked. See `rows_at`.
        """
        rows = self.group(row)
        if rows == self._row_table_for and self._row_table is not None:
            return self._row_table
        ws = workspace_of(ctx)
        if self._row_table is not None:
            ws.delete_node(self._row_table)
            self._row_table = None
        sliced = self.frame.rows_slice(rows)
        if sliced is None:
            self._row_table_for = None
            return None
        self._row_table = ws.insert_node_dyn(sliced)
        # No frame of its own: it is seated inside a card that is already one,
        # and two borders a pixel apart read as a mistake rather than a nesting.
        ws.submit_action(self._row_table, dex.SetTableBordered(False),
                         "Seated the records in the readout")
        self._row_table_for = list(rows)
        return self._row_table

    def draw_overlay(self, ctx, x, y, w, h, row):
        """The whole record, pinned to the bottom-right: the readout, and under
        it the rows behind the mark as a table."""
        mark = self._local_pos.get(row)
        if mark is not None:
            ring(ctx, mark[0], mark[1], POINT_R + 6.0, LINE, 2.0)
        rows = self.group(row)
        paint_selection(ctx, x, y, w, h,
                        selection_lines(row, len(rows), self.point_label(row)),
                        self.row_table(ctx, row), len(rows))


# ======================================================================
# Axes
# ======================================================================
#
# Shared because a scatter, a bar chart, a strip and a violin all want the same
# left gutter of values and the same foot of categories, and because two views
# that draw their axes differently are two views that are hard to read together.


class ValueAxis:
    """A continuous axis: a mapping, and — when asked — the drawing of one.

    The two are separate because the drawing does not always belong to the view.
    On a plane, the grid and the tick captions are a *background*, worked out
    for whatever range is currently visible and redrawn as you pan; the view
    only wants the mapping. Off a plane there is nowhere else for them to go, so
    the view draws them itself.
    """

    def __init__(self, x, y, w, h, lo, hi, step, title):
        self.top = y + TOP
        self.bottom = y + h - FOOT
        self.left = x + GUTTER
        self.right = x + w
        self.lo = lo
        self.hi = hi
        self.step = step
        self.title = title
        self.span = (hi - lo) if hi > lo else 1.0

    def to_y(self, value):
        return self.bottom - (value - self.lo) / self.span * (self.bottom - self.top)

    def to_value(self, y):
        """The inverse: what a position on the axis is worth.

        What lets a background work out which values are on screen, and so put
        ticks where the reader is actually looking rather than where the whole
        of the data would have put them.
        """
        height = self.bottom - self.top
        if height == 0.0:
            return self.lo
        return self.lo + (self.bottom - y) / height * self.span

    def width(self):
        return self.right - self.left

    def scale(self):
        """This axis as plain data, for `PlotScale` to hand to a background."""
        return {
            "kind": "value",
            "lo": self.lo,
            "hi": self.hi,
            "step": self.step,
            "title": self.title,
            "near": self.bottom,
            "far": self.top,
        }

    def paint(self, ctx):
        """The gridlines, the captions and the rule — the view drawing its own."""
        for tick in axis_ticks(self.lo, self.hi, self.step):
            gy = self.to_y(tick)
            line(ctx, [(self.left, gy), (self.right, gy)], GRID, 1.0)
            caption = tick_text(tick, self.step)
            (cw, ch) = measure(ctx, caption, TICK_FONT)
            text(ctx, caption, self.left - 6.0 - cw, gy - ch / 2.0, TICK_FONT, FAINT)
        line(ctx, [(self.left, self.top), (self.left, self.bottom)], AXIS, 1.2)


def value_axis(ctx, plot, x, y, w, h, values, title):
    """The `ValueAxis` covering `values`, published and drawn if it is the
    view's to draw."""
    (lo, hi, step) = nice_bounds(min(values), max(values)) if values else (0.0, 1.0, 0.5)
    axis = ValueAxis(x, y, w, h, lo, hi, step, title)
    plot.publish_axis(ctx, y_axis=axis)
    return axis


def category_scale(labels, p0, p1, title):
    """A categorical axis as plain data: `n` slots evenly across `[p0, p1]`."""
    return {
        "kind": "category",
        "labels": [str(v) for v in labels],
        "p0": p0,
        "p1": p1,
        "title": title,
    }


#: The fewest characters of a category name worth showing. Below this a caption
#: says nothing that hovering the bar does not say better.
MIN_CAPTION_CHARS = 3


def captions_fit(ctx, labels, slot):
    """Whether every one of these will fit in a slot that wide.

    All or none, deliberately. Truncating each to whatever fits turns a crowded
    axis into a row of stumps that look like words and are not — and dropping
    only *some* leaves a scattering that reads as if the rest were missing
    rather than as if none were shown. When they do not fit, nothing is drawn
    and the mark itself is hovered or clicked to name it.
    """
    room = slot - 4.0
    if room <= 0.0:
        return False
    for label in labels:
        label = str(label)
        shortest = label[:MIN_CAPTION_CHARS]
        if measure(ctx, shortest, TICK_FONT)[0] > room:
            return False
    return True


def names_fit(ctx, labels, slot):
    """Whether every one of these fits a slot that wide *whole*.

    The test for a caption that will not be truncated. `captions_fit` asks only
    whether a stump would fit, which is right where a stump is what gets drawn —
    a category axis, where "Torger" is visibly the front of a longer word. It is
    wrong where the name is written out in full and the reader has no way to
    tell: "Species 1" is not a shortening of "Species 12", it is a different
    tip, and drawing one for the other is worse than drawing neither.
    """
    room = slot - 4.0
    if room <= 0.0:
        return False
    return all(measure(ctx, str(label), TICK_FONT)[0] <= room for label in labels)


def captions_stack(ctx, spacing):
    """Whether one caption per slot can be read when the slots are `spacing`
    apart *across* the line each is written on.

    The width test `captions_fit` makes is the wrong one for captions laid down
    a page or fanned around a ring: those are as wide as there is room for, and
    what runs out is the gap between one line of text and the next. All or none,
    for the same reason as `captions_fit`.
    """
    return spacing >= TICK_FONT + 2.0


def x_caption(ctx, caption, cx, baseline, slot):
    """A category caption under the axis, truncated to the slot it has."""
    caption = str(caption)
    while caption and measure(ctx, caption, TICK_FONT)[0] > slot - 4.0 and len(caption) > 1:
        caption = caption[:-1]
    (cw, _) = measure(ctx, caption, TICK_FONT)
    text(ctx, caption, cx - cw / 2.0, baseline + 6.0, TICK_FONT, INK)


def stats_card(ctx, x, y, w, lines):
    """A small panel of numbers pinned to the plot's top-right."""
    if not lines:
        return
    width = max(measure(ctx, s, STAT_FONT)[0] for s in lines) + 16.0
    height = len(lines) * (STAT_FONT + 5.0) + 8.0
    left = x + w - width - 4.0
    top = y + TOP
    rect(ctx, left, top, width, height, PANEL, PANEL_EDGE, 4.0, 235)
    for (k, s) in enumerate(lines):
        text(ctx, s, left + 8.0, top + 5.0 + k * (STAT_FONT + 5.0), STAT_FONT, INK)


# ======================================================================
# Cartesian layouts
# ======================================================================


class Scatter(Plot):
    """Two continuous columns, a point per row, with the least-squares line."""

    KIND = "scatter"
    CHANNELS = ("x", "y", "color")
    WANTS_PLANE = True
    #: Whether the least-squares line is drawn and reported. A sample of
    #: measurements has a fit worth showing; something worked out from a formula
    #: has no residuals, and a regression line through it is a claim about
    #: scatter where there is none.
    FIT = True
    # See `Plot.become`.
    connect = None

    def __init__(self, frame, sensor, connect=None, **encoding):
        super().__init__(frame, sensor, **encoding)
        #: Join the points in order of this channel — `"x"`, `"y"`, or `None`
        #: for a cloud. A line through measurements invents an order they do not
        #: have; a line through a sampled function is what the function *is*,
        #: and the points are only where it was looked at.
        self.connect = connect if connect in ("x", "y") else None

    def paint(self, ctx, x, y, w, h):
        (xc, yc) = (self.enc["x"], self.enc["y"])
        rows = [i for i in range(self.frame.n)
                if is_num(self.frame.value(xc, i)) and is_num(self.frame.value(yc, i))]
        if not rows:
            text(ctx, "nothing numeric to plot", x + GUTTER, y + TOP, BAR_FONT, FAINT)
            return
        xs = [float(self.frame.value(xc, i)) for i in rows]
        ys = [float(self.frame.value(yc, i)) for i in rows]
        (ylo, yhi, ystep) = nice_bounds(min(ys), max(ys))
        (xlo, xhi, xstep) = nice_bounds(min(xs), max(xs))
        axis = ValueAxis(x, y, w, h, ylo, yhi, ystep, yc)
        xspan = (xhi - xlo) if xhi > xlo else 1.0

        def to_x(v):
            return axis.left + (v - xlo) / xspan * axis.width()

        across = {
            "kind": "value", "lo": xlo, "hi": xhi, "step": xstep, "title": xc,
            "near": axis.left, "far": axis.right,
        }
        self.publish_axis(ctx, x_axis=across, y_axis=axis)
        if self.chrome:
            for tick in axis_ticks(xlo, xhi, xstep):
                caption = tick_text(tick, xstep)
                text(ctx, caption,
                     to_x(tick) - measure(ctx, caption, TICK_FONT)[0] / 2.0,
                     axis.bottom + 6.0, TICK_FONT, FAINT)
            text(ctx, xc, axis.right - measure(ctx, xc, TICK_FONT)[0],
                 axis.bottom + 18.0, TICK_FONT, INK)

        pairs = list(zip(xs, ys))
        fit = least_squares(pairs) if self.FIT else None
        if fit is not None:
            (a, b) = fit
            line(ctx, [(to_x(xlo), axis.to_y(a * xlo + b)),
                       (to_x(xhi), axis.to_y(a * xhi + b))], LINE, 1.6)
        if self.connect and len(pairs) > 1:
            # In order of whichever channel the points were sampled along, so a
            # curve lying on its side joins up the way it was drawn.
            ordered = sorted(pairs, key=(lambda p: p[0]) if self.connect == "x"
                             else (lambda p: p[1]))
            line(ctx, [(to_x(a), axis.to_y(b)) for (a, b) in ordered], LINE, 1.6)

        inks = self.point_inks(rows)
        for (k, i) in enumerate(rows):
            (cx, cy) = (to_x(xs[k]), axis.to_y(ys[k]))
            dot(ctx, cx, cy, POINT_R, inks[k])
            self.record(ctx, i, cx, cy)

        stats = ["n = %d" % len(rows)]
        r = pearson(pairs) if self.FIT else None
        if r is not None:
            stats.append("r = %+.3f" % r)
        if fit is not None:
            stats.append("slope = %.3g" % fit[0])
        self.publish_stats(ctx, x, y, w, stats)


class Bars(Plot):
    """One categorical column, as how often each category occurs."""

    KIND = "bars"
    CHANNELS = ("x",)

    def paint(self, ctx, x, y, w, h):
        column = self.enc["x"]
        (levels, counts, per_row) = self.frame.tally(column)
        if not levels:
            text(ctx, "no categories to count", x + GUTTER, y + TOP, BAR_FONT, FAINT)
            return
        (lo, hi, step) = nice_bounds(0, max(counts))
        axis = ValueAxis(x, y, w, h, 0.0, hi, step, "count of %s" % column)
        slot = axis.width() / len(levels)
        named = captions_fit(ctx, levels, slot)
        self.publish_axis(
            ctx,
            x_axis=category_scale(levels, axis.left, axis.right, column),
            y_axis=axis,
        )
        bar_w = min(slot * 0.7, 80.0)
        for (j, level) in enumerate(levels):
            cx = axis.left + slot * (j + 0.5)
            top = axis.to_y(counts[j])
            ink = category_color(j)
            rect(ctx, cx - bar_w / 2.0, top, bar_w, axis.bottom - top, ink)
            if self.chrome and named:
                x_caption(ctx, level, cx, axis.bottom, slot)
            caption = str(counts[j])
            text(ctx, caption, cx - measure(ctx, caption, TICK_FONT)[0] / 2.0,
                 top - 14.0, TICK_FONT, INK)
            # Every row of this category sits at the bar's top-centre, so a link
            # from another view still lands on it.
            for i in per_row[level]:
                self.record(ctx, i, cx, top)

    def rows_at(self, row):
        """The whole category, because that is what a bar is.

        Every row of it sits at the same point, so a click picks up an arbitrary
        one of them; the bar was what was clicked, and the bar is these rows.
        """
        column = self.enc.get("x")
        value = self.frame.value(column, row) if column else None
        if value is None:
            return super().rows_at(row)
        return [i for i in range(self.frame.n)
                if self.frame.value(column, i) == value]


class Strip(Plot):
    """A continuous column as a point per row, jittered sideways.

    Split by a categorical `x` if one is given, with a quartile box over each
    group; otherwise one band of everything.
    """

    KIND = "strip"
    CHANNELS = ("x", "y")
    #: Whether a group also gets a density curve around it. `Violin` says yes.
    VIOLIN = False

    def groups(self):
        """`[(label, [(row, value)])]` — the bands this view draws."""
        value_col = self.enc.get("y") or self.enc.get("x")
        group_col = self.enc.get("x") if self.enc.get("y") else None
        if group_col is None or self.frame.is_continuous(group_col):
            rows = [(i, float(self.frame.value(value_col, i)))
                    for i in range(self.frame.n)
                    if is_num(self.frame.value(value_col, i))]
            return (value_col, [(value_col, rows)] if rows else [])
        out = []
        for level in self.frame.levels(group_col):
            rows = [(i, float(self.frame.value(value_col, i)))
                    for i in self.frame.tally(group_col)[2][level]
                    if is_num(self.frame.value(value_col, i))]
            if rows:
                out.append((level, rows))
        return (value_col, out)

    def paint(self, ctx, x, y, w, h):
        (value_col, groups) = self.groups()
        if not groups:
            text(ctx, "nothing numeric to plot", x + GUTTER, y + TOP, BAR_FONT, FAINT)
            return
        every = [v for (_, rows) in groups for (_, v) in rows]
        (lo, hi, step) = nice_bounds(min(every), max(every))
        axis = ValueAxis(x, y, w, h, lo, hi, step, value_col)
        slot = axis.width() / len(groups)
        named = captions_fit(ctx, [label for (label, _) in groups], slot)
        self.publish_axis(
            ctx,
            x_axis=category_scale([label for (label, _) in groups],
                                  axis.left, axis.right,
                                  self.enc.get("x") if len(groups) > 1 else ""),
            y_axis=axis,
        )
        half = min(slot * 0.3, 40.0)
        for (j, (label, rows)) in enumerate(groups):
            cx = axis.left + slot * (j + 0.5)
            ink = category_color(j)
            values = [v for (_, v) in rows]
            if self.VIOLIN:
                self.paint_violin(ctx, axis, cx, half, values, ink)
            else:
                (q1, med, q3) = quartiles(values)
                rect(ctx, cx - half, axis.to_y(q3), 2.0 * half,
                     axis.to_y(q1) - axis.to_y(q3), tint(ink))
                line(ctx, [(cx - half, axis.to_y(med)), (cx + half, axis.to_y(med))],
                     ink, 1.6)
            for (i, v) in rows:
                jx = cx + self._jit[i] * half * 0.75
                cy = axis.to_y(v)
                dot(ctx, jx, cy, POINT_R, ink)
                self.record(ctx, i, jx, cy)
            if len(groups) > 1 and self.chrome and named:
                x_caption(ctx, label, cx, axis.bottom, slot)
            mean = sum(values) / len(values)
            caption = "μ %.4g" % mean
            text(ctx, caption, cx - measure(ctx, caption, TICK_FONT)[0] / 2.0,
                 axis.top - 2.0, TICK_FONT, INK)

        self.publish_stats(
            ctx, x, y, w,
            ["%d group%s" % (len(groups), "" if len(groups) == 1 else "s"),
             "n = %d" % len(every)])

    def paint_violin(self, ctx, axis, cx, half, values, ink):
        """The density of `values`, mirrored about `cx`.

        Drawn behind the points rather than instead of them: the curve says what
        the shape of the column is, the points say how many rows made it and
        where the odd ones are.
        """
        (lo, hi) = (min(values), max(values))
        if hi <= lo:
            line(ctx, [(cx - half, axis.to_y(lo)), (cx + half, axis.to_y(lo))], ink, 1.6)
            return
        pad = (hi - lo) * 0.08
        grid = [lo - pad + (hi - lo + 2.0 * pad) * k / (VIOLIN_STEPS - 1.0)
                for k in range(VIOLIN_STEPS)]
        density = gaussian_kde(values, grid)
        peak = max(density) or 1.0
        right = [(cx + half * d / peak, axis.to_y(v)) for (v, d) in zip(grid, density)]
        left = [(cx - half * d / peak, axis.to_y(v)) for (v, d) in zip(grid, density)]
        polygon(ctx, right + list(reversed(left)), tint(ink, t=0.62),
                dex.Stroke.new(1.2, dex.Color.rgb(*ink)))
        (q1, med, q3) = quartiles(values)
        rect(ctx, cx - 2.0, axis.to_y(q3), 4.0, axis.to_y(q1) - axis.to_y(q3), shade(ink))
        line(ctx, [(cx - half * 0.5, axis.to_y(med)), (cx + half * 0.5, axis.to_y(med))],
             PANEL, 1.8)


class Violin(Strip):
    """A strip with the column's density drawn around each band."""

    KIND = "violin"
    VIOLIN = True
    WANTS_PLANE = True


class Heatmap(Plot):
    """Two categorical columns, as the contingency table between them."""

    KIND = "heatmap"
    CHANNELS = ("x", "y")

    def paint(self, ctx, x, y, w, h):
        (xc, yc) = (self.enc["x"], self.enc["y"])
        xlevels = self.frame.levels(xc)
        ylevels = self.frame.levels(yc)
        if not xlevels or not ylevels:
            text(ctx, "no categories to cross", x + GUTTER, y + TOP, BAR_FONT, FAINT)
            return
        xi = {v: j for (j, v) in enumerate(xlevels)}
        yi = {v: j for (j, v) in enumerate(ylevels)}
        counts = [[0] * len(xlevels) for _ in ylevels]
        cell_rows = {}
        total = 0
        for i in range(self.frame.n):
            (xv, yv) = (self.frame.value(xc, i), self.frame.value(yc, i))
            if xv in xi and yv in yi:
                counts[yi[yv]][xi[xv]] += 1
                cell_rows.setdefault((yi[yv], xi[xv]), []).append(i)
                total += 1
        peak = max((c for row in counts for c in row), default=0) or 1

        grid_x = x + GUTTER
        top = y + TOP
        bottom = y + h - FOOT
        cell_w = (x + w - grid_x) / len(xlevels)
        cell_h = (bottom - top) / len(ylevels)
        self.publish_axis(
            ctx,
            x_axis=category_scale(xlevels, grid_x, x + w, xc),
            y_axis=category_scale(ylevels, bottom, top, yc),
        )

        for (r, ylevel) in enumerate(ylevels):
            cy0 = top + r * cell_h
            if self.chrome and cell_h >= TICK_FONT + 2.0:
                (lw, lh) = measure(ctx, str(ylevel), TICK_FONT)
                text(ctx, str(ylevel), grid_x - 6.0 - lw,
                     cy0 + cell_h / 2.0 - lh / 2.0, TICK_FONT, FAINT)
            for (c, xlevel) in enumerate(xlevels):
                cx0 = grid_x + c * cell_w
                count = counts[r][c]
                rect(ctx, cx0 + 1.0, cy0 + 1.0, cell_w - 2.0, cell_h - 2.0,
                     lerp_rgb(HEAT_LOW, HEAT_HIGH, count / peak))
                if count and cell_w > 22.0 and cell_h > 16.0:
                    caption = str(count)
                    (cw, ch) = measure(ctx, caption, TICK_FONT)
                    text(ctx, caption, cx0 + cell_w / 2.0 - cw / 2.0,
                         cy0 + cell_h / 2.0 - ch / 2.0, TICK_FONT,
                         PANEL if count / peak > 0.55 else INK)
                centre = (cx0 + cell_w / 2.0, cy0 + cell_h / 2.0)
                for i in cell_rows.get((r, c), []):
                    self.record(ctx, i, centre[0], centre[1])

        if self.chrome and captions_fit(ctx, xlevels, cell_w):
            for (c, xlevel) in enumerate(xlevels):
                x_caption(ctx, xlevel, grid_x + c * cell_w + cell_w / 2.0, bottom, cell_w)

        v = cramers_v(counts, len(ylevels), len(xlevels), total)
        stats = ["n = %d" % total]
        if v is not None:
            stats.append("Cramér's V = %.3f" % v)
        self.publish_stats(ctx, x, y, w, stats)

    def rows_at(self, row):
        """The whole cell: every row crossing the same pair of levels.

        A cell is a count, and the count is these rows — which is the only thing
        a heatmap leaves you wanting, because the number in the square says how
        many and nothing at all about which.
        """
        (xc, yc) = (self.enc.get("x"), self.enc.get("y"))
        if not xc or not yc:
            return super().rows_at(row)
        (xv, yv) = (self.frame.value(xc, row), self.frame.value(yc, row))
        if xv is None or yv is None:
            return super().rows_at(row)
        return [i for i in range(self.frame.n)
                if self.frame.value(xc, i) == xv and self.frame.value(yc, i) == yv]


# ======================================================================
# Building a view, and putting it on a plane
# ======================================================================


#: The encoding channels. Anything else passed to `build_plot` is a layout's own
#: option — a tree's `shape` or `parent`, a circos's `tracks` — and goes to its
#: constructor.
CHANNEL_NAMES = ("x", "y", "z", "color", "key")


def build_plot(ws, layout, source=None, frame=None, **encoding):
    """A view of `layout` over `source`, ready to be inserted or drawn.

    The sensor a view picks with needs a workspace to live in, which a bare
    constructor has none of — so this is how a view is made. Pass `frame` to
    share one already-read table between several views rather than reading it
    again for each.

        scatter = build_plot(dex.ws, Scatter, table, x="mass", y="flipper")

    Anything unnamed is filled in from the table: the first two continuous
    columns for a scatter, the first categorical one for bars, and so on, so a
    view built with no encoding at all still opens onto something.
    """
    frame = frame if frame is not None else Frame(source)
    extra = {k: v for (k, v) in encoding.items() if k not in CHANNEL_NAMES}
    channels = {k: v for (k, v) in encoding.items() if k in CHANNEL_NAMES}
    channels = default_encoding(frame, layout, channels)
    # Hover and click; drag only for a layout that says it wants it, because on
    # a plane the drag is how the surface pans.
    sensor = ws.insert_node_dyn(
        dex.InteractionBox.sensing(True, True, layout.SENSES_DRAG))
    return layout(frame, sensor, **dict(channels, **extra))


def default_encoding(frame, layout, given):
    """Fill in whatever channels `given` left out, from what the table has."""
    out = {k: v for (k, v) in given.items() if v}
    continuous = frame.continuous()
    categorical = frame.categorical()
    kind = getattr(layout, "KIND", "plot")

    def pick(pool, taken):
        for column in pool:
            if column not in taken:
                return column
        return pool[0] if pool else (frame.columns[0] if frame.columns else None)

    if kind in ("bars", "heatmap"):
        wanted = categorical or frame.columns
    else:
        wanted = continuous or frame.columns
    if not out.get("x"):
        out["x"] = pick(wanted, ())
    if "y" in layout.CHANNELS and not out.get("y"):
        # A strip and a violin split a continuous column *by* a categorical one,
        # so their x is the grouping and their y is the measure.
        pool = continuous if kind in ("strip", "violin") else wanted
        out["y"] = pick(pool, (out.get("x"),))
        if kind in ("strip", "violin") and categorical:
            out["x"] = pick(categorical, ())
    if "z" in layout.CHANNELS and not out.get("z"):
        out["z"] = pick(continuous, (out.get("x"), out.get("y")))
    return out


def on_plane(ws, body, size, name=None, foreground=(), what="Placed it",
             background=()):
    """Put `body` on a pan/zoom canvas of its own, and return the canvas.

    Every view here is happier bigger than the box it is shown in: drawn once,
    life-size and generous, and moved around by dragging and alt-scrolling. That
    way no view has to invent its own navigation, and none of them has to
    reflow — a scatter with ten thousand points is a big picture, not a small
    one with everything on top of everything else.

    What goes in `foreground` stays put and life-size while the plane moves
    under it, which is where a readout belongs: legible at any magnification.
    """
    canvas = dex.Canvas.build(ws)
    item = dex.StaticCanvasItem.build(
        ws, body, dex.Vector.new(0.0, 0.0), dex.Vector.new(size[0], size[1]))
    ws.submit_action(canvas, dex.AdoptCanvasNode(item, dex.Layer.midground()), what)
    for node in background:
        ws.submit_action(canvas, dex.AdoptCanvasNode(node, dex.Layer.background()),
                         "Added the grid")
    for node in foreground:
        ws.submit_action(canvas, dex.AdoptCanvasNode(node, dex.Layer.foreground()),
                         "Added the chrome")
    # A plane is a means, not an end: the inspector's heading and every crumb in
    # the trail should say what is on it, not that it is a canvas.
    if name:
        ws.submit_action(canvas, dex.NameCanvas(name=name), "Named the plane")
    return canvas


def adopt(ws, canvas, node, layer):
    """Put `node` on `canvas`, and hand back its id.

    For chrome that has to *ask* the plane where it has been panned to: it needs
    the plane's id to be built at all, and the plane does not exist until
    `on_plane` has made it. So it is adopted afterwards rather than passed in.
    """
    uid = ws.insert_node_dyn(node)
    ws.submit_action(canvas, dex.AdoptCanvasNode(uid, layer), "Added the chrome")
    return uid


#: How big a view is drawn on its own plane, before any magnification.
PLANE_SIZE = (1100.0, 820.0)


def plot_on_plane(ws, plot, size=PLANE_SIZE, name=None):
    """A view on a plane of its own, with everything around it drawn separately.

    The view stops drawing its own chrome and three nodes on the plane take it
    over: the grid and the tick captions behind it, worked out for whatever is
    on screen; its title and its readout in front. All three stay the size they
    were written at however far the plane is zoomed, which a picture drawn once
    at a fixed size cannot.
    """
    plot.chrome = False
    body = ws.insert_node_dyn(plot)
    canvas = on_plane(ws, body, size, name or plot.type_name())
    adopt(ws, canvas, PlotAxes(canvas, body), dex.Layer.background())
    adopt(ws, canvas, PlotTitle(canvas, body), dex.Layer.foreground())
    adopt(ws, canvas, PlotStatsPanel(canvas, body), dex.Layer.foreground())
    adopt(ws, canvas, PlotChrome(body), dex.Layer.foreground())
    return canvas


# ======================================================================
# A plane's chrome
# ======================================================================
#
# What a view does *not* draw, once it is on a plane: the grid, the axes, the
# captions and the title. All three are separate nodes on the plane rather than
# marks in the picture, and each is that way for its own reason.
#
# The grid and the captions, because a picture drawn once at a fixed size stops
# being a plot the moment you zoom: ten ticks become two, or two hundred. A
# background asks the view how it maps data onto the plane, works out which
# values are actually on screen, and puts ticks *there* — so magnifying a
# crowded corner subdivides the axis instead of stretching it.
#
# The title, because it is a label rather than part of the picture: it should
# stay legible at any magnification instead of scaling away to nothing.

#: How far a caption sits from the edge of the viewport it is pinned to.
RULER_INSET = 6.0
#: The most gridlines drawn across one axis, however far out the plane is zoomed.
MAX_GRIDLINES = 60


def axis_position(axis, value):
    """Where `value` falls on a continuous axis, in the plane's coordinates."""
    span = (axis["hi"] - axis["lo"]) or 1.0
    return axis["near"] + (value - axis["lo"]) / span * (axis["far"] - axis["near"])


def axis_value(axis, position):
    """The inverse: what a position on a continuous axis is worth."""
    reach = axis["far"] - axis["near"]
    if reach == 0.0:
        return axis["lo"]
    span = (axis["hi"] - axis["lo"]) or 1.0
    return axis["lo"] + (position - axis["near"]) / reach * span


class PlaneChrome:
    """What the nodes below share: a view, and the mapping onto its plane.

    Neither the canvas nor the view is *owned* here — a script node owns only
    what its `owned_nodes` says — so both are references, rewritten to the
    copies when the plane is deep-cloned.
    """

    def __init__(self, canvas, plot):
        self.canvas = canvas
        self.plot = plot

    def owned_nodes(self):
        return []

    def view(self, ctx):
        """`(origin, zoom, base, width, height)`, or `None` if there is no plane.

        A background is drawn in the surface's *own* frame rather than on the
        panned layer, so it maps its own points: a plane point `p` lands at
        `base.pos + (p - origin) * zoom`. Which is the whole of the arithmetic
        below, and the reason this is not just drawn into the picture.
        """
        ws = ctx.node.workspace
        origin = ws.send_request(self.canvas, dex.CanvasViewOrigin())
        if origin is None:
            return None
        zoom = ws.send_request(self.canvas, dex.CanvasZoom()) or 1.0
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        h = base.y.provided_value() if base.y is not None else None
        if w is None or h is None or zoom <= 0.0:
            return None
        return (origin, zoom, base, w, h)

    def scale(self, ctx):
        """How the view maps data onto the plane, or `None` if it does not."""
        return ctx.node.workspace.send_request(self.plot, PlotScale())

    def nothing(self):
        return dex.DrawResult.Complete(region=None)


class PlotAxes(PlaneChrome):
    """The grid and the tick captions, worked out for what is on screen.

    A gridline travels with the plane, because it marks a place in the data. A
    caption does not: it rides along its own axis with the line it names, but
    sits a fixed distance from the edge of the viewport and at a fixed size, so
    whatever is on screen always says what it is.
    """

    def type_name(self):
        return "Plot Axes"

    def draw(self, ctx):
        view = self.view(ctx)
        scale = self.scale(ctx)
        if view is None or not scale:
            return self.nothing()
        (origin, zoom, base, w, h) = view

        # The plane window currently on screen, in the plane's own coordinates.
        window = {
            "x": (origin.x, origin.x + w / zoom),
            "y": (origin.y, origin.y + h / zoom),
        }
        for (channel, axis) in scale.items():
            if channel not in window:
                continue
            if axis.get("kind") == "category":
                self.paint_categories(ctx, base, origin, zoom, w, h, channel, axis)
            else:
                self.paint_values(ctx, base, origin, zoom, w, h, channel,
                                  axis, window[channel])
        return self.nothing()

    def visible_ticks(self, axis, window):
        """Nice tick values covering just the part of the axis on screen.

        Over the *visible* range, not the whole of the data: that is what makes
        zooming in subdivide the axis rather than stretch it.
        """
        (a, b) = (axis_value(axis, window[0]), axis_value(axis, window[1]))
        (lo, hi) = (min(a, b), max(a, b))
        (lo, hi, step) = nice_bounds(lo, hi)
        ticks = axis_ticks(lo, hi, step)
        return (ticks[:MAX_GRIDLINES], step)

    def paint_values(self, ctx, base, origin, zoom, w, h, channel, axis, window):
        (ticks, step) = self.visible_ticks(axis, window)
        for value in ticks:
            at_plane = axis_position(axis, value)
            caption = tick_text(value, step)
            (cw, ch) = measure(ctx, caption, TICK_FONT)
            if channel == "y":
                sy = base.pos.y + (at_plane - origin.y) * zoom
                if not (base.pos.y <= sy <= base.pos.y + h):
                    continue
                line(ctx, [(base.pos.x, sy), (base.pos.x + w, sy)], GRID, 1.0)
                text(ctx, caption, base.pos.x + RULER_INSET, sy - ch - 1.0,
                     TICK_FONT, FAINT)
            else:
                sx = base.pos.x + (at_plane - origin.x) * zoom
                if not (base.pos.x <= sx <= base.pos.x + w):
                    continue
                line(ctx, [(sx, base.pos.y), (sx, base.pos.y + h)], GRID, 1.0)
                text(ctx, caption, sx + 2.0,
                     base.pos.y + h - ch - RULER_INSET, TICK_FONT, FAINT)
        self.paint_title(ctx, base, w, h, channel, axis.get("title"))

    def paint_categories(self, ctx, base, origin, zoom, w, h, channel, axis):
        """One caption per category, at the middle of the band it was given."""
        labels = axis.get("labels") or []
        if not labels:
            return
        (p0, p1) = (axis["p0"], axis["p1"])
        slot = abs(p1 - p0) / len(labels) * zoom
        if not captions_fit(ctx, labels, slot):
            self.paint_title(ctx, base, w, h, channel, axis.get("title"))
            return
        slot = (p1 - p0) / len(labels)
        for (i, label) in enumerate(labels):
            at_plane = p0 + slot * (i + 0.5)
            (cw, ch) = measure(ctx, label, TICK_FONT)
            if channel == "y":
                sy = base.pos.y + (at_plane - origin.y) * zoom
                if not (base.pos.y <= sy <= base.pos.y + h):
                    continue
                text(ctx, label, base.pos.x + RULER_INSET, sy - ch / 2.0,
                     TICK_FONT, FAINT)
            else:
                sx = base.pos.x + (at_plane - origin.x) * zoom
                if not (base.pos.x <= sx <= base.pos.x + w):
                    continue
                text(ctx, label, sx - cw / 2.0,
                     base.pos.y + h - ch - RULER_INSET, TICK_FONT, FAINT)
        self.paint_title(ctx, base, w, h, channel, axis.get("title"))

    def paint_title(self, ctx, base, w, h, channel, title):
        """The column an axis is showing, pinned to the middle of its edge."""
        if not title:
            return
        (cw, ch) = measure(ctx, title, TICK_FONT, bold=True)
        if channel == "y":
            text(ctx, title, base.pos.x + RULER_INSET, base.pos.y + RULER_INSET,
                 TICK_FONT, INK, bold=True)
        else:
            text(ctx, title, base.pos.x + (w - cw) / 2.0,
                 base.pos.y + h - ch - RULER_INSET, TICK_FONT, INK, bold=True)


class PlotTitle(PlaneChrome):
    """What the view is showing, over the top of it.

    In the foreground and at a fixed size, because a title is a label rather
    than part of the picture: drawn into the plane it would shrink away with
    everything else the moment anybody zoomed out.
    """

    def type_name(self):
        return "Plot Title"

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        h = base.y.provided_value() if base.y is not None else None
        if w is None or h is None:
            return self.nothing()
        ws = ctx.node.workspace
        caption = ws.send_request(self.plot, PlotName())
        if not caption:
            return self.nothing()
        (cw, ch) = measure(ctx, caption, TITLE_FONT, bold=True)
        left = base.pos.x + (w - cw) / 2.0
        top = base.pos.y + RULER_INSET
        rect(ctx, left - 8.0, top - 3.0, cw + 16.0, ch + 6.0, PANEL, None, 4.0, 214)
        text(ctx, caption, left, top, TITLE_FONT, INK, bold=True)
        return self.nothing()


class PlotStatsPanel(PlaneChrome):
    """The view's figures, in front of it.

    A panel of numbers is a reading of the data rather than part of it, so it
    holds its size and its corner while the picture moves under it. Which is
    also the only way it stays readable: drawn into the plane it would be
    illegible zoomed out and enormous zoomed in.
    """

    def type_name(self):
        return "Plot Figures"

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        if w is None:
            return self.nothing()
        lines = ctx.node.workspace.send_request(self.plot, PlotStats()) or []
        if lines:
            stats_card(ctx, base.pos.x, base.pos.y - TOP, w, lines)
        return self.nothing()


class PlotChrome:
    """A view's readout, pinned in the plane's foreground.

    It holds the view only by id and asks it the protocol's own questions —
    which is the point: the chrome is an ordinary consumer of the same messages
    any other view would use, so if it can draw a readout, so can anything else.
    """

    def __init__(self, plot):
        self.plot = plot

    def owned_nodes(self):
        return []

    def type_name(self):
        return "A Plot Readout"

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        h = base.y.provided_value() if base.y is not None else None
        if w is None or h is None or not (math.isfinite(w) and math.isfinite(h)):
            return dex.DrawResult.Complete(region=None)
        (x, y) = (base.pos.x, base.pos.y)
        ws = ctx.node.workspace

        hovered = ws.send_request(self.plot, HoverRow())
        selected = ws.send_request(self.plot, Selection())
        if hovered is not None:
            self.draw_hover(ctx, ws, x, y, w, h, hovered)
        if selected is not None:
            self.draw_row(ctx, ws, x, y, w, h, selected)
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))

    def draw_hover(self, ctx, ws, x, y, w, h, row):
        """What the pointer is over, beside the mark it is over.

        Beside the mark rather than in a corner: a readout in a fixed corner
        makes you look away from the thing you are pointing at to read what it
        is, and then back again to check you are still on it. The view answers
        `DrawnPoint` in screen coordinates, so the card can follow the mark
        wherever the plane has moved it — while staying its own size, which is
        why it is drawn here and not down in the picture.
        """
        label = ws.send_request(self.plot, PointLabel(row))
        at_screen = ws.send_request(self.plot, DrawnPoint(row))
        if not label or at_screen is None:
            return
        (mx, my) = to_local(ctx, at_screen)
        lines = ["row %d" % row] + label.split(", ")
        (cw, ch) = card_size(ctx, lines, READOUT_FONT)
        ring(ctx, mx, my, POINT_R + 4.0, INK, 1.5)
        # Up and to the right of the mark, folded back inside the viewport at
        # its edges so it is never half off the screen.
        left = min(max(mx + 12.0, x + 4.0), x + w - cw - 4.0)
        top = min(max(my - ch - 10.0, y + 4.0), y + h - ch - 4.0)
        card(ctx, left, top, cw, ch, lines, READOUT_FONT, INK, PANEL_EDGE)

    def draw_row(self, ctx, ws, x, y, w, h, row):
        """What was selected, along the bottom: the readout, then the records.

        The table is asked of the view rather than built here — the view is what
        holds the source, and a second copy of the records would be a second
        thing to keep in step. So is how many rows the mark stood for: a click
        on a bar picked up a category, and only the view knows that.
        """
        rows = ws.send_request(self.plot, RowGroup(row)) or [row]
        paint_selection(
            ctx, x, y, w, h,
            selection_lines(row, len(rows),
                            ws.send_request(self.plot, PointLabel(row))),
            ws.send_request(self.plot, RowTable(row)), len(rows))


# ======================================================================
# Trees
# ======================================================================
#
# One layout, two shapes. A hierarchy is the same tree whether it is drawn down
# a page or around a circle — the same parents, the same order of leaves, the
# same row at the end of every branch — so it is one node with a `shape`, not
# two that would drift apart.

#: Fields that end a lineage rather than becoming a rank of their own.
UNPLACED = ("unclassified", "unassigned", "na", "environmental samples", "")

DOT_MIN = 3.0
DOT_MAX = 8.0
BRANCH_W = 1.6
BRANCH = (26, 138, 92)

#: The gap a circular tree leaves open, so the first and last leaf do not meet.
OPEN_ANGLE = 0.28
START_ANGLE = -math.pi / 2.0 + OPEN_ANGLE / 2.0


def split_lineage(value):
    """A lineage string as a list of names, in order from the root.

    Semicolon-separated fields, trimmed, with rank prefixes (`d__`, `p__`) taken
    off — the same lineage should make the same tree whether or not whoever
    wrote it labelled the ranks. An unplaced rank ends the lineage rather than
    becoming a name of its own: everything unclassified is not one clade.
    """
    out = []
    for field in str(value).split(";"):
        field = field.strip()
        if len(field) > 3 and field[1:3] == "__":
            field = field[3:].strip()
        if field.lower() in UNPLACED:
            break
        out.append(field)
    return out


class Tree:
    """A hierarchy, however the table happened to write one down.

    Built once and kept: which node is under which, where the leaves fall in
    order, and — the part that matters for the protocol — which node each table
    row ends on, so a row can be found on the tree and the tree can say which
    row a branch belongs to.

    Two constructors, because tables carry trees two ways. `Tree(lineages)`
    reads a column of `"A;B;C"` strings, which is what a taxonomy dump looks
    like. `Tree.from_edges(...)` reads one row per node with a column pointing
    at its parent, which is what a phylogenetics tool emits and the only shape
    that can carry branch lengths. Everything downstream — the layout, the
    picking, the protocol — is written against what they both produce.
    """

    def __init__(self, lineages):
        self.nodes = {}
        self.roots = []
        self.row_leaf = {}
        for (row, value) in lineages:
            names = split_lineage(value)
            if not names:
                continue
            path = ()
            parent = None
            for (depth, name) in enumerate(names):
                path = path + (name,)
                node = self.nodes.get(path)
                if node is None:
                    node = {"key": path, "name": name, "depth": depth,
                            "parent": parent, "children": [], "rows": [], "weight": 0}
                    self.nodes[path] = node
                    if parent is None:
                        self.roots.append(path)
                    else:
                        self.nodes[parent]["children"].append(path)
                node["weight"] += 1
                parent = path
            self.nodes[path]["rows"].append(row)
            self.row_leaf[row] = path
        self.max_depth = max((n["depth"] for n in self.nodes.values()), default=0)
        self.heaviest = max((n["weight"] for n in self.nodes.values()), default=0)
        self.leaves = self._leaves()
        self.slot = self._slots()

    @classmethod
    def from_edges(cls, ids, parents, depths=None, leaf_order=None):
        """A tree from one row per node and a column naming each one's parent.

        `depths` are cumulative distances from the root — branch lengths, to
        scale — and stand in for the rank count when they are given; without
        them depth is counted in hops. `leaf_order` fixes where the tips fall
        around the ring, which a phylogenetics tool has already decided and
        which a re-derivation would quietly disagree with.

        A row *is* a node here, rather than ending on one, so every row is
        placeable and `DrawnPoints` answers for the internal nodes too.
        """
        tree = cls.__new__(cls)
        tree.nodes = {}
        tree.roots = []
        tree.row_leaf = {}

        row_of = {}
        for (row, node) in enumerate(ids):
            if node is None or node in row_of:
                continue
            row_of[node] = row
            tree.nodes[(node,)] = {
                "key": (node,), "name": str(node), "depth": 0,
                "parent": None, "children": [], "rows": [row], "weight": 1,
            }
            tree.row_leaf[row] = (node,)

        for (row, node) in enumerate(ids):
            if node not in row_of or row_of[node] != row:
                continue
            parent = parents[row] if row < len(parents) else None
            if parent is None or parent not in row_of or parent == node:
                tree.roots.append((node,))
            else:
                tree.nodes[(node,)]["parent"] = (parent,)
                tree.nodes[(parent,)]["children"].append((node,))

        # Depth: the distance column if there is one, else hops from the root.
        if depths is not None and any(is_num(d) for d in depths):
            for (node, row) in row_of.items():
                value = depths[row] if row < len(depths) else None
                tree.nodes[(node,)]["depth"] = float(value) if is_num(value) else 0.0
        else:
            stack = [(key, 0) for key in tree.roots]
            while stack:
                (key, depth) = stack.pop()
                tree.nodes[key]["depth"] = depth
                for child in tree.nodes[key]["children"]:
                    stack.append((child, depth + 1))

        # Weight: how many rows sit at or under a node, so a dot can be sized.
        for key in sorted(tree.nodes, key=lambda k: tree.nodes[k]["depth"], reverse=True):
            tree.nodes[key]["weight"] = 1 + sum(
                tree.nodes[c]["weight"] for c in tree.nodes[key]["children"])

        tree.max_depth = max((n["depth"] for n in tree.nodes.values()), default=0)
        tree.heaviest = max((n["weight"] for n in tree.nodes.values()), default=0)
        tree.leaves = tree._leaves()
        # The tool's own tip order wins where it is given: it decided, and a
        # second opinion here would only disagree with the rest of its output.
        if leaf_order is not None and any(is_num(v) for v in leaf_order):
            tree.leaves.sort(key=lambda key: (
                leaf_order[row_of[key[0]]]
                if is_num(leaf_order[row_of[key[0]]]) else row_of[key[0]]))
        tree.slot = tree._slots()
        return tree

    def _leaves(self):
        """Every childless node, depth-first — the order they spread out in."""
        out = []
        stack = list(reversed(self.roots))
        while stack:
            key = stack.pop()
            node = self.nodes[key]
            if node["children"]:
                stack.extend(reversed(node["children"]))
            else:
                out.append(key)
        return out

    def _slots(self):
        """A slot for every node: leaves in order, parents centred over theirs.

        Worked out bottom-up over the *structure*, not by sorting on depth.
        Depth is a distance when the table carries branch lengths, and a
        distance need not grow from a parent to its child — a zero-length branch
        gives them the same one, and a rounded column can give the child less.
        Ordering the work by depth therefore reached some parents before their
        children and asked for a slot that had not been worked out yet.

        Anything the roots cannot reach — a parent column with a cycle in it —
        is placed after everything else rather than left without a slot, so a
        malformed tree draws and shows the problem instead of failing to draw.
        """
        slot = {key: float(i) for (i, key) in enumerate(self.leaves)}
        # Pre-order, iteratively, because a real tree is deeper than the
        # recursion limit; reversed, it visits every child before its parent.
        order = []
        seen = set(self.roots)
        stack = list(self.roots)
        while stack:
            key = stack.pop()
            order.append(key)
            for child in self.nodes[key]["children"]:
                if child not in seen:
                    seen.add(child)
                    stack.append(child)
        for key in reversed(order):
            children = self.nodes[key]["children"]
            if children:
                slot[key] = sum(slot[c] for c in children) / len(children)

        stranded = [key for key in self.nodes if key not in slot]
        for (i, key) in enumerate(stranded):
            slot[key] = float(len(self.leaves) + i)
        return slot

    def dot_radius(self, weight):
        """A dot sized by the rows under it, on a square-root scale so the
        *area* tracks the count rather than the width."""
        if self.heaviest <= 0:
            return DOT_MIN
        share = max(0.0, min(1.0, weight / self.heaviest))
        return DOT_MIN + (DOT_MAX - DOT_MIN) * math.sqrt(share)


class Phylogeny(Plot):
    """A tree of the lineages in one column, down the page or around a circle.

    Every table row is placed at the node its lineage ends on, so `DrawnPoints`
    answers for rows exactly as a scatter does — which is the whole point: a
    tree and a scatter built from one table can be laid side by side and joined
    record by record, without either knowing what the other is.
    """

    KIND = "phylogeny"
    CHANNELS = ("x", "color")

    #: The shapes it can be drawn in.
    SHAPES = ("hierarchical", "sideways", "circular")
    # Declared on the class as well as set in `__init__`, so a view that
    # `become`s a tree finds these rather than missing them. See `Plot.become`.
    shape = "hierarchical"
    parent_col = None
    depth_col = None
    order_col = None
    #: Replaced the moment the columns are read; here so an instance that
    #: `become`s a tree has one before `on_encoding_changed` runs.
    tree = Tree([])

    def __init__(self, frame, sensor, shape="hierarchical",
                 parent=None, depth=None, leaf_order=None, **encoding):
        #: Naming a parent column switches the source from lineage strings to
        #: an edge list; the others only mean anything alongside it.
        self.parent_col = parent if parent in frame.columns else None
        self.depth_col = depth if depth in frame.columns else None
        self.order_col = leaf_order if leaf_order in frame.columns else None
        super().__init__(frame, sensor, **encoding)
        #: `"hierarchical"` down the page, `"sideways"` across it as a
        #: dendrogram, or `"circular"` around a centre.
        self.shape = shape if shape in self.SHAPES else "hierarchical"
        self.tree = Tree([])
        self.on_encoding_changed()

    def on_encoding_changed(self):
        """The tree is derived from columns, so it is rebuilt when they change."""
        column = self.enc.get("x")
        if column is None:
            self.tree = Tree([])
        elif self.parent_col:
            self.tree = Tree.from_edges(
                self.frame.values(column),
                self.frame.values(self.parent_col),
                self.frame.values(self.depth_col) if self.depth_col else None,
                self.frame.values(self.order_col) if self.order_col else None,
            )
        else:
            self.tree = Tree(list(enumerate(self.frame.values(column))))

    def type_name(self):
        return "A %s Tree of %s" % (self.shape.title(), self.enc.get("x"))

    def ranks(self):
        """How many ranks deep the tree goes — what a rank axis bands."""
        return max(self.tree.max_depth, 1) if self.tree else 1

    def point_label(self, row):
        leaf = self.tree.row_leaf.get(row) if self.tree else None
        if leaf is None:
            return super().point_label(row)
        if self.parent_col:
            # One row per node: its own name says everything the path would.
            return "%s: %s" % (self.enc.get("x"), leaf[0])
        return "%s: %s" % (self.enc.get("x"), " › ".join(str(n) for n in leaf))

    def rows_at(self, row):
        """Every row ending on the same node of the tree.

        A tree's mark is a *node*, and several records can end on one: the same
        lineage written twice is one tip, not two, and the tip is both of them.
        """
        leaf = self.tree.row_leaf.get(row) if self.tree else None
        if leaf is None or leaf not in self.tree.nodes:
            return super().rows_at(row)
        return list(self.tree.nodes[leaf]["rows"])

    def node_ink(self, key):
        """A node's colour: by the clade it falls in, if a colour column says so."""
        column = self.enc.get("color")
        if not column:
            return BRANCH
        rows = self.tree.nodes[key]["rows"]
        if not rows:
            return BRANCH
        levels = self.frame.levels(column)
        palette = spread_palette(len(levels))
        value = self.frame.value(column, rows[0])
        return palette[levels.index(value)] if value in levels else BRANCH

    def paint(self, ctx, x, y, w, h):
        if not self.tree or not self.tree.nodes:
            text(ctx, "no lineages to draw", x + MARGIN, y + MARGIN, BAR_FONT, FAINT)
            return
        if self.shape == "circular":
            self.paint_circular(ctx, x, y, w, h)
        else:
            self.paint_cartesian(ctx, x, y, w, h)

    # -- down the page, or across it -------------------------------------

    def depth_axis(self, x, y, w, h):
        """Where depth runs, and where the leaves spread.

        The only difference between the two cartesian shapes: one puts depth
        down the page and leaves across it, the other the other way round. Every
        elbow, dot and caption below is written once against this.
        """
        if self.shape == "sideways":
            # Room on the left for the root's caption, and on the right for the
            # leaves', which are the long ones.
            return ((x + GUTTER, x + w - CIRCULAR_LABEL_ROOM),
                    (y + MARGIN, y + h - MARGIN), False)
        return ((y + TOP, y + h - FOOT), (x + MARGIN, x + w - MARGIN), True)

    def paint_cartesian(self, ctx, x, y, w, h):
        """A dendrogram: square elbows from a parent out to a bus and back in to
        each child, with depth running whichever way the shape says."""
        tree = self.tree
        ((d0, d1), (s0, s1), down) = self.depth_axis(x, y, w, h)
        span = max(len(tree.leaves) - 1, 1)
        ranks = max(tree.max_depth, 1)

        def place(key):
            depth = d0 + (d1 - d0) * (tree.nodes[key]["depth"] / ranks)
            across = s0 + (s1 - s0) * (tree.slot[key] / span)
            return (across, depth) if down else (depth, across)

        step = (d1 - d0) / ranks * 0.5
        for key in tree.nodes:
            node = tree.nodes[key]
            if not node["children"]:
                continue
            here = place(key)
            kids = [place(c) for c in node["children"]]
            if down:
                bus = here[1] + step
                line(ctx, [here, (here[0], bus)], BRANCH, BRANCH_W)
                line(ctx, [(min(k[0] for k in kids), bus),
                           (max(k[0] for k in kids), bus)], BRANCH, BRANCH_W)
                for kid in kids:
                    line(ctx, [(kid[0], bus), kid], BRANCH, BRANCH_W)
            else:
                bus = here[0] + step
                line(ctx, [here, (bus, here[1])], BRANCH, BRANCH_W)
                line(ctx, [(bus, min(k[1] for k in kids)),
                           (bus, max(k[1] for k in kids))], BRANCH, BRANCH_W)
                for kid in kids:
                    line(ctx, [(bus, kid[1]), kid], BRANCH, BRANCH_W)

        # All the tips or none of them, as a crowded category axis is captioned —
        # but on the *whole* name, because a tip's is written out in full and a
        # trimmed one reads as a different tip rather than as a trimmed one.
        slot = abs(s1 - s0) / span
        names = [tree.nodes[leaf]["name"] for leaf in tree.leaves]
        named = names_fit(ctx, names, slot) if down else captions_stack(ctx, slot)
        self.paint_nodes(ctx, place, horizontal=down, named=named)

    # -- around a circle -------------------------------------------------

    def paint_circular(self, ctx, x, y, w, h):
        """The same tree bent round: depth is radius, order is angle, and a
        branch is an arc along its parent's radius plus a spoke out to the child."""
        tree = self.tree
        (cx, cy) = (x + w / 2.0, y + h / 2.0)
        outer = min(w, h) / 2.0 - MARGIN - CIRCULAR_LABEL_ROOM
        inner = outer * 0.16
        span = max(len(tree.leaves) - 1, 1)
        rows = max(tree.max_depth, 1)
        step = (2.0 * math.pi - OPEN_ANGLE) / span

        def angle(key):
            return START_ANGLE + step * tree.slot[key]

        def radius(key):
            return inner + (outer - inner) * (tree.nodes[key]["depth"] / rows)

        def place(key):
            (a, r) = (angle(key), radius(key))
            return (cx + r * math.cos(a), cy + r * math.sin(a))

        for key in tree.nodes:
            node = tree.nodes[key]
            if not node["children"]:
                continue
            r = radius(key)
            kid_angles = [angle(c) for c in node["children"]]
            # The arc along this node's own radius, spanning its children...
            arc = [(cx + r * math.cos(a), cy + r * math.sin(a))
                   for a in _sweep(min(kid_angles), max(kid_angles))]
            line(ctx, arc, BRANCH, BRANCH_W)
            # ...and a spoke out to each of them.
            for (child, a) in zip(node["children"], kid_angles):
                kr = radius(child)
                line(ctx, [(cx + r * math.cos(a), cy + r * math.sin(a)),
                           (cx + kr * math.cos(a), cy + kr * math.sin(a))],
                     BRANCH, BRANCH_W)

        # Round the ring the tips fan out, so what they have between them is
        # the arc from one spoke to the next at the outermost radius.
        self.paint_nodes(ctx, place, horizontal=False, angle=angle,
                         named=captions_stack(ctx, step * outer))

    # -- what both shapes share ------------------------------------------

    def paint_nodes(self, ctx, place, horizontal, angle=None, named=True):
        """A dot at every node, a caption on every leaf, and every row recorded
        at the node its lineage ends on.

        `named` is off when the tips are too close together to write on: a wall
        of overlapping names is not a denser reading of the tree, it is an
        unreadable one, and the tip is still named by hovering it.
        """
        tree = self.tree
        for key in tree.nodes:
            node = tree.nodes[key]
            (px, py) = place(key)
            ink = self.node_ink(key)
            dot(ctx, px, py, tree.dot_radius(node["weight"]), ink)
            for row in node["rows"]:
                self.record(ctx, row, px, py)
            if node["children"] or not named:
                continue
            caption = node["name"]
            (cw, ch) = measure(ctx, caption, TICK_FONT)
            if angle is None:
                if horizontal:
                    text(ctx, caption, px - cw / 2.0, py + DOT_MAX + 2.0,
                         TICK_FONT, INK)
                else:
                    # Across the page, so the caption goes beside the dot where
                    # there is room for a long name rather than under it.
                    text(ctx, caption, px + DOT_MAX + 4.0, py - ch / 2.0, TICK_FONT, INK)
            else:
                # Outside the ring, pushed off along its own spoke so the
                # captions fan out instead of piling up at the top and bottom.
                a = angle(key)
                text(ctx, caption,
                     px + math.cos(a) * 6.0 - (cw / 2.0 if abs(math.cos(a)) < 0.3 else
                                               (cw if math.cos(a) < 0 else 0.0)),
                     py + math.sin(a) * 6.0 - ch / 2.0, TICK_FONT, INK)


#: How much room a circular tree leaves outside the ring for leaf captions.
CIRCULAR_LABEL_ROOM = 74.0


def _sweep(a0, a1, step=0.03):
    """The angles from `a0` to `a1`, close enough together to read as an arc."""
    if a1 <= a0:
        return [a0]
    count = max(2, int((a1 - a0) / step) + 1)
    return [a0 + (a1 - a0) * k / (count - 1.0) for k in range(count)]


# ======================================================================
# Circos
# ======================================================================
#
# A ring of sectors — one per level of a categorical column — with the rows laid
# out around it and a track per continuous column outside. What a circos is for
# is comparing a lot of series against one position, which a row of stacked
# strips does badly and a ring does well.

#: How far a curve may stray from the true arc before it is subdivided again.
CURVE_TOL = 0.25
#: The gap between one sector and the next, in radians.
SECTOR_GAP = 0.012


def polar(cx, cy, r, a):
    return (cx + r * math.cos(a), cy + r * math.sin(a))


def arc_points(cx, cy, r, a0, a1):
    """An arc, subdivided just finely enough that it does not read as a polygon."""
    if r <= 0.0:
        return [polar(cx, cy, r, a0), polar(cx, cy, r, a1)]
    theta = math.sqrt(8.0 * CURVE_TOL / r)
    steps = max(2, int(abs(a1 - a0) / theta) + 2)
    return [polar(cx, cy, r, a0 + (a1 - a0) * i / (steps - 1.0)) for i in range(steps)]


def sector_points(cx, cy, r_in, r_out, a0, a1):
    """A ring segment: out along one arc and back along the other."""
    return arc_points(cx, cy, r_out, a0, a1) + arc_points(cx, cy, r_in, a1, a0)


def chord_points(cx, cy, r, a0, a1, bow=0.35):
    """A ribbon from one point on the ring to another, bowed through the middle.

    Straight lines across a circos read as a cat's cradle; bowing them toward
    the centre is what lets a hundred of them still show where they go.
    """
    (x0, y0) = polar(cx, cy, r, a0)
    (x1, y1) = polar(cx, cy, r, a1)
    (mx, my) = (cx + (x0 + x1 - 2.0 * cx) * bow / 2.0,
                cy + (y0 + y1 - 2.0 * cy) * bow / 2.0)
    out = []
    for k in range(CHORD_STEPS):
        t = k / (CHORD_STEPS - 1.0)
        u = 1.0 - t
        # A quadratic Bézier through the bowed midpoint.
        out.append((u * u * x0 + 2.0 * u * t * mx + t * t * x1,
                    u * u * y0 + 2.0 * u * t * my + t * t * y1))
    return out


#: How many points a chord is drawn with.
CHORD_STEPS = 24


class Circos(Plot):
    """Rows around a ring, grouped into sectors, with a track per column.

    `x` is the categorical column that makes the sectors. Every continuous
    column named in `tracks` becomes a ring outside them, drawn as a bar per row
    against that track's own scale. `link` names a column holding another row's
    key, and each one becomes a chord across the middle.
    """

    KIND = "circos"
    CHANNELS = ("x", "color")
    # See `Plot.become`.
    tracks = ()
    link = None

    def __init__(self, frame, sensor, tracks=(), link=None, **encoding):
        super().__init__(frame, sensor, **encoding)
        #: The continuous columns drawn as rings, innermost first.
        self.tracks = [c for c in tracks if c in frame.columns]
        #: A column naming another row's key, drawn as a chord.
        self.link = link if link in frame.columns else None

    def type_name(self):
        return "A Circos of %s" % (self.enc.get("x") or "nothing")

    def sectors(self):
        """`[(level, a0, a1, rows)]` — the ring, divided by the grouping column.

        Each sector is as wide as the rows in it, so the ring is a picture of
        the whole table and a row's angle means the same thing in every track.
        """
        column = self.enc.get("x")
        if not column:
            return [(None, START_ANGLE, START_ANGLE + 2.0 * math.pi,
                     list(range(self.frame.n)))]
        (levels, counts, per_row) = self.frame.tally(column)
        total = sum(counts) or 1
        out = []
        angle = 0.0
        for (level, count) in zip(levels, counts):
            width = 2.0 * math.pi * count / total
            out.append((level, angle + SECTOR_GAP / 2.0,
                        angle + width - SECTOR_GAP / 2.0, per_row[level]))
            angle += width
        return out

    def paint(self, ctx, x, y, w, h):
        (cx, cy) = (x + w / 2.0, y + h / 2.0)
        outer = min(w, h) / 2.0 - MARGIN - CIRCULAR_LABEL_ROOM
        if outer <= 20.0:
            return
        band = 26.0
        ring_room = min(len(self.tracks) * band, outer * 0.45)
        r_out = outer - ring_room
        r_in = max(r_out - 22.0, outer * 0.3)

        angle_of = {}
        for (i, (level, a0, a1, rows)) in enumerate(self.sectors()):
            ink = category_color(i)
            polygon(ctx, sector_points(cx, cy, r_in, r_out, a0, a1), ink)
            # Rows spread evenly across their own sector, in table order.
            for (k, row) in enumerate(rows):
                a = a0 + (a1 - a0) * ((k + 0.5) / max(len(rows), 1))
                angle_of[row] = a
                (px, py) = polar(cx, cy, (r_in + r_out) / 2.0, a)
                self.record(ctx, row, px, py)
            if level is not None:
                self.paint_sector_label(ctx, cx, cy, r_out, (a0 + a1) / 2.0, level)

        for (t, column) in enumerate(self.tracks):
            self.paint_track(ctx, cx, cy, r_out + t * band + 4.0, band - 6.0,
                             column, angle_of, category_color(t + 3))
        if self.link:
            self.paint_links(ctx, cx, cy, r_in, angle_of)

    def paint_sector_label(self, ctx, cx, cy, r, a, level):
        caption = str(level)
        (cw, ch) = measure(ctx, caption, TICK_FONT)
        (px, py) = polar(cx, cy, r + 8.0, a)
        text(ctx, caption,
             px - (cw / 2.0 if abs(math.cos(a)) < 0.3 else (cw if math.cos(a) < 0 else 0.0)),
             py - ch / 2.0, TICK_FONT, INK)

    def paint_track(self, ctx, cx, cy, r0, thickness, column, angle_of, ink):
        """One ring: a bar per row, against this column's own range.

        Its own range, not a shared one — a track is read against itself, and
        forcing two columns onto one scale is how a circos stops saying anything.
        """
        values = [v for v in self.frame.values(column) if is_num(v)]
        if not values:
            return
        (lo, hi) = (min(values), max(values))
        span = (hi - lo) if hi > lo else 1.0
        line(ctx, arc_points(cx, cy, r0, 0.0, 2.0 * math.pi), GRID, 1.0)
        for (row, a) in angle_of.items():
            v = self.frame.value(column, row)
            if not is_num(v):
                continue
            height = thickness * (float(v) - lo) / span
            polygon(ctx, sector_points(cx, cy, r0, r0 + height,
                                       a - TRACK_BAR / 2.0, a + TRACK_BAR / 2.0), ink)
        (cw, ch) = measure(ctx, column, TICK_FONT)
        text(ctx, column, cx - cw / 2.0, cy - r0 - thickness - ch, TICK_FONT, FAINT)

    def paint_links(self, ctx, cx, cy, r, angle_of):
        """A chord for every row whose link column names another row."""
        key_col = self.enc.get("key") or self.link
        index = self.frame.rows_by_key(key_col) if key_col else {}
        for row in range(self.frame.n):
            target = index.get(self.frame.value(self.link, row))
            if target is None or target == row:
                continue
            (a0, a1) = (angle_of.get(row), angle_of.get(target))
            if a0 is None or a1 is None:
                continue
            line(ctx, chord_points(cx, cy, r, a0, a1), LINE, 1.0)


#: How wide one row's bar is in a track, in radians.
TRACK_BAR = 0.008


# ======================================================================
# Three dimensions
# ======================================================================
#
# A projection, not a renderer: three columns onto the plane, turned by
# dragging. Which is enough to see the shape of a cloud that a pair of 2D
# scatters flattens away — and, because it records where every point landed
# like any other layout, a 3D view links to a 2D one exactly as two 2D views do.


def turn(point, yaw, tilt):
    """A point `(x, y, z)` in unit space, spun by `yaw` then `tilt`."""
    (x, y, z) = point
    (cy, sy) = (math.cos(yaw), math.sin(yaw))
    (x, z) = (x * cy - z * sy, x * sy + z * cy)
    (ct, st) = (math.cos(tilt), math.sin(tilt))
    (y, z) = (y * ct - z * st, y * st + z * ct)
    return (x, y, z)


class Scatter3D(Plot):
    """Three continuous columns, projected, and turned by dragging.

    Depth does the work colour usually would: a point further away is drawn
    smaller and paler, which is what stops the cloud reading as a flat smear.
    Points are painted back to front so the near ones win.
    """

    KIND = "scatter3d"
    CHANNELS = ("x", "y", "z", "color")
    #: This one takes the drag. A plane it sits on gives up panning in exchange
    #: for the view being turned by the same gesture.
    SENSES_DRAG = True
    # See `Plot.become`.
    yaw = 0.6
    tilt = -0.35

    def __init__(self, frame, sensor, **encoding):
        super().__init__(frame, sensor, **encoding)
        #: Where the camera is. View state: mutated in `draw`, never through an
        #: action, so turning the cloud does not fill the undo history.
        self.yaw = 0.6
        self.tilt = -0.35

    def unit_points(self):
        """`[(row, (x, y, z))]` with each axis scaled into `[-1, 1]`.

        Scaled per axis rather than together: three columns in three different
        units have no shared scale, and pretending they do just hides whichever
        has the smallest numbers.
        """
        cols = [self.enc.get(c) for c in ("x", "y", "z")]
        if not all(cols):
            return []
        rows = [i for i in range(self.frame.n)
                if all(is_num(self.frame.value(c, i)) for c in cols)]
        if not rows:
            return []
        spans = []
        for column in cols:
            values = [float(self.frame.value(column, i)) for i in rows]
            (lo, hi) = (min(values), max(values))
            spans.append((lo, (hi - lo) or 1.0))
        out = []
        for i in rows:
            unit = tuple(
                2.0 * (float(self.frame.value(cols[k], i)) - spans[k][0]) / spans[k][1] - 1.0
                for k in range(3)
            )
            out.append((i, unit))
        return out

    def paint(self, ctx, x, y, w, h):
        points = self.unit_points()
        if not points:
            text(ctx, "three numeric columns are needed", x + MARGIN, y + MARGIN,
                 BAR_FONT, FAINT)
            return
        (cx, cy) = (x + w / 2.0, y + h / 2.0)
        scale = min(w, h) / 2.0 - MARGIN - 20.0

        self.paint_frame(ctx, cx, cy, scale)
        inks = self.point_inks([row for (row, _) in points])
        # Back to front, so a near point covers a far one rather than the reverse.
        turned = [(row, turn(unit, self.yaw, self.tilt)) for (row, unit) in points]
        order = sorted(range(len(turned)), key=lambda k: turned[k][1][2])
        for k in order:
            (row, (px, py, pz)) = turned[k]
            depth = (pz + 1.7) / 3.4
            sx = cx + px * scale
            sy = cy + py * scale
            dot(ctx, sx, sy, POINT_R * (0.55 + 0.55 * depth),
                lerp_rgb(PANEL, inks[k], 0.35 + 0.65 * depth))
            self.record(ctx, row, sx, sy)

        text(ctx, " / ".join(self.enc[c] for c in ("x", "y", "z")),
             x + MARGIN, y + MARGIN, TICK_FONT, FAINT)
        self.publish_stats(ctx, x, y, w, ["n = %d" % len(points), "drag to turn"])

    def paint_frame(self, ctx, cx, cy, scale):
        """The unit cube, so the cloud has something to be turned against."""
        corners = [(sx, sy, sz)
                   for sx in (-1.0, 1.0) for sy in (-1.0, 1.0) for sz in (-1.0, 1.0)]
        seen = [turn(c, self.yaw, self.tilt) for c in corners]
        for (i, a) in enumerate(corners):
            for (j, b) in enumerate(corners):
                if j <= i:
                    continue
                # An edge is a pair differing in exactly one coordinate.
                if sum(1 for k in range(3) if a[k] != b[k]) != 1:
                    continue
                line(ctx, [(cx + seen[i][0] * scale, cy + seen[i][1] * scale),
                           (cx + seen[j][0] * scale, cy + seen[j][1] * scale)],
                     GRID, 1.0)

    def interact(self, ctx, x, y, w, h):
        """The shared picking, plus the drag that turns the cloud."""
        super().interact(ctx, x, y, w, h)
        drag = ctx.node.workspace.send_request(self.sensor, dex.WasDragged())
        if drag is not None:
            self.yaw += drag.x * TURN_RATE
            self.tilt = max(-1.4, min(1.4, self.tilt + drag.y * TURN_RATE))


#: Radians turned per pixel dragged.
TURN_RATE = 0.008


#: Every layout, by the `kind` its `Encoding` reports. What `SetEncoding(kind=…)`
#: looks a name up in, and the whole of what a view needs to become another one.
LAYOUTS_BY_KIND = {
    layout.KIND: layout
    for layout in (Scatter, Bars, Strip, Violin, Heatmap, Phylogeny, Circos, Scatter3D)
}


# ======================================================================
# Running a lambda, many times over
# ======================================================================
#
# Sampling a function means evaluating it hundreds of times. Wiring a value in
# and reading the output back cannot do that: it computes once, on a worker,
# and the answer arrives a frame later. And re-running a lambda the ordinary
# way would run this whole prelude again for every sample.
#
# So a runner is *prepared* once from the graph and then called. A script
# lambda's source is compiled once and exec'd per call. A canvas lambda is read
# into its graph of operator lambdas, each of those compiled once, and evaluated
# by walking it. Both memoise: sampling ground already covered costs nothing,
# which is what makes panning a plotted equation cheap.


class Unrunnable(Exception):
    """A lambda that cannot be evaluated, and why."""


class LambdaRunner:
    """A lambda, prepared for being run over and over.

        f = LambdaRunner(dex.snapshot, equation)
        ys = [f(x=v) for v in xs]

    Holds no node ids beyond the one it was built from: everything it needs is
    read out of the graph once, up front. So it keeps working while the sampling
    goes on, and stops reflecting the graph if the graph is edited — which is
    the trade a cache always makes, and why `params` and the source are read
    again whenever the plot rebuilds.
    """

    #: How deep a chain of lambdas may nest before it is called a loop.
    MAX_DEPTH = 64

    def __init__(self, snap, uid):
        self.uid = uid
        self.params = [name for (name, _port, _src)
                       in (snap.send_request(uid, dex.DataflowInputs()) or [])]
        self.body = snap.send_request(uid, dex.LambdaBody())
        self.pins = snap.send_request(uid, dex.ParamPins()) or []
        #: `{node: (kind, payload)}`, read once so a call touches no graph.
        self.plan = {}
        #: Compiled scripts, by node.
        self.code = {}
        #: Answers, by the arguments that produced them.
        self.cache = {}
        if self.body is None:
            self.source = snap.send_request(uid, dex.ActiveScript())
            if not self.source:
                raise Unrunnable("that lambda has no script to run")
            self.code[uid] = compile(self.source, "<lambda>", "exec")
        else:
            self.source = None
            self._plan(snap, self.body, 0)

    # -- reading the graph, once -----------------------------------------

    def _plan(self, snap, node, depth):
        """Work out what produces `node`, and everything under it."""
        if node in self.plan or node in self.pins:
            return
        if depth > self.MAX_DEPTH:
            raise Unrunnable("that graph nests too deeply to run")

        found = self._producer(snap, node)
        if found is None:
            raise Unrunnable("a wire in that lambda leads nowhere")
        (kind, target) = found
        if kind == "pin":
            return
        if kind == "const":
            self.plan[node] = ("const", target)
            return

        script = snap.send_request(target, dex.ActiveScript())
        if not script:
            raise Unrunnable("an operator in that lambda has no script")
        self.code[target] = compile(script, "<operator>", "exec")
        inputs = snap.send_request(target, dex.DataflowInputs()) or []
        wired = []
        for (name, _port, source) in inputs:
            if source is None:
                raise Unrunnable("an operator in that lambda has an unwired input")
            wired.append((name, source))
            self._plan(snap, source, depth + 1)
        self.plan[node] = ("call", (target, wired))

    def _producer(self, snap, node):
        """What actually makes the value on this wire.

        A wire points at an output slot, a canvas item, or a pin; only one of
        those computes anything, and the others have to be followed through.
        """
        for _ in range(self.MAX_DEPTH):
            if node in self.pins:
                return ("pin", node)
            owner = snap.owner_of(node)
            if owner is not None \
                    and snap.send_request(owner, dex.DataflowOutput()) == node:
                node = owner
                continue
            if snap.send_request(node, dex.ActiveScript()):
                return ("lambda", node)
            child = snap.send_request(node, dex.CanvasNodeChild())
            if child is not None:
                node = child
                continue
            text = snap.send_request(node, dex.GetText())
            if text is not None:
                return ("const", text)
            return None
        return None

    # -- running it, many times ------------------------------------------

    def __call__(self, **bindings):
        key = tuple(sorted(bindings.items()))
        if key in self.cache:
            return self.cache[key]
        if self.body is None:
            value = self._run(self.uid, bindings)
        else:
            pinned = {pin: bindings.get(name)
                      for (pin, name) in zip(self.pins, self.params)}
            value = self._value(self.body, pinned, 0)
        self.cache[key] = value
        return value

    def _value(self, node, pinned, depth):
        if node in pinned:
            return pinned[node]
        if depth > self.MAX_DEPTH:
            raise Unrunnable("that graph nests too deeply to run")
        step = self.plan.get(node)
        if step is None:
            raise Unrunnable("that lambda changed since it was prepared")
        (kind, payload) = step
        if kind == "const":
            return as_number(payload)
        (target, wired) = payload
        return self._run(target, {name: self._value(source, pinned, depth + 1)
                                  for (name, source) in wired})

    def _run(self, node, bindings):
        """One script, with its arguments bound, run for its `transform`."""
        namespace = dict(bindings)
        namespace["dex"] = dex
        exec(self.code[node], namespace)
        transform = namespace.get("transform")
        if transform is None:
            raise Unrunnable("a script in that lambda defines no `transform`")
        return transform()


def as_number(value):
    """A wire's constant as a number, or `None` if it is not one."""
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


# ======================================================================
# What the protocol is for
# ======================================================================
#
# Two operations that work on *any* pair of views, because they are written
# against the messages and not against any layout. If a new layout answers the
# protocol, both of these work on it the day it is written.


def view_of(ws, node):
    """The view that answers the protocol at `node`, through a plane if need be.

    A join is between *views* — things that can say where they drew each row and
    what is in it. A plane (a `Canvas`) is not one; it carries one. And a plane
    is exactly what a data explorer or a phylogeny hands back, so connecting two
    of those means reaching through each to the single view on it. A node that
    already answers the protocol is its own view, so this is a no-op for a bare
    plot — which is what makes it safe to fold into every join below.
    """
    if ws.send_request(node, Encoding()) is not None:
        return node
    for item in (ws.send_request(node, dex.CanvasChildren()) or []):
        child = ws.send_request(item, dex.CanvasNodeChild())
        if child is not None and ws.send_request(child, Encoding()) is not None:
            return child
    return node


def row_correspondence(ws, left, right):
    """`[(left_row, right_row)]` — the records the two views have in common.

    By row id when both were built from the same table, which is the common
    case and needs nothing declared. Otherwise by the key column each view names
    in its `Encoding`: two different tables, joined on a shared accession or
    name rather than on position, which position would get wrong.
    """
    (left, right) = (view_of(ws, left), view_of(ws, right))
    left_enc = ws.send_request(left, Encoding()) or {}
    right_enc = ws.send_request(right, Encoding()) or {}
    (lk, rk) = (left_enc.get("key"), right_enc.get("key"))
    if not lk or not rk:
        shared = set(ws.send_request(left, RowKeys()) or ()) \
            & set(ws.send_request(right, RowKeys()) or ())
        return [(row, row) for row in sorted(shared)]

    left_values = ws.send_request(left, SourceTable()).column(lk).to_pylist()
    right_index = {}
    for (i, v) in enumerate(ws.send_request(right, SourceTable()).column(rk).to_pylist()):
        right_index.setdefault(v, i)
    out = []
    for (i, v) in enumerate(left_values):
        j = right_index.get(v)
        if j is not None:
            out.append((i, j))
    return out


class Correspondence:
    """Lines between the same record in two views, wherever each drew it.

    Neither view knows the other exists. This asks both where they put every
    row this frame and draws between the pairs — which is why it works for a
    scatter beside a tree, or a 3D cloud beside a circos, on the same plane or
    on two different ones.

    It must be drawn *after* both, and paints in screen coordinates, so it
    belongs somewhere over the top of them both: a plane's foreground, or a
    layout holding the pair.
    """

    def __init__(self, left, right, ink=LINE, width=0.8):
        self.left = left
        self.right = right
        self.ink = tuple(ink)
        self.width = width
        #: Draw only the selected record, rather than every one of them. A
        #: hundred lines is a picture; ten thousand is a grey rectangle.
        self.only_selected = False

    def owned_nodes(self):
        return []

    def type_name(self):
        return "A Correspondence"

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        h = base.y.provided_value() if base.y is not None else None
        draw_links(ctx, self.left, self.right, self.ink, self.width,
                   self.only_selected)
        if w is None or h is None or not (math.isfinite(w) and math.isfinite(h)):
            return dex.DrawResult.Complete(region=None)
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))


def draw_links(ctx, left, right, ink=LINE, width=0.8, only_selected=False):
    """Draw a line between the same record in two views, wherever each put it.

    The whole of the join, and it is written against the protocol and nothing
    else: ask both where they drew every row *this frame*, work out which rows
    they have in common, and connect the pairs. Neither view is told the other
    exists, which is why a scatter joins to a tree as readily as to another
    scatter.

    Both views must already have drawn this frame — the answers are a record of
    where they put things, not a promise about where they will.
    """
    ws = ctx.node.workspace
    # Through a plane to the view on it: what is wired in is often the plane a
    # data explorer or a phylogeny handed back, and a plane cannot say where it
    # drew a row. `row_correspondence` reaches through the same way.
    (left, right) = (view_of(ws, left), view_of(ws, right))
    here = ws.send_request(left, DrawnPoints()) or {}
    there = ws.send_request(right, DrawnPoints()) or {}
    if not here or not there:
        return
    pairs = row_correspondence(ws, left, right)
    if only_selected:
        chosen = ws.send_request(left, Selection())
        pairs = [(a, b) for (a, b) in pairs if a == chosen]
    for (a, b) in pairs:
        (p, q) = (here.get(a), there.get(b))
        if p is not None and q is not None:
            # Back into the drawing node's own space: the answers are in screen
            # coordinates, and that node may itself be on a plane.
            line(ctx, [to_local(ctx, p), to_local(ctx, q)], ink, width)


def to_local(ctx, point):
    """A screen point in this node's own drawing coordinates."""
    mapped = ctx.from_global(dex.ScreenPos.new(point[0], point[1]))
    return (mapped.x, mapped.y)


def link_views(ws, left, right, only_selected=False):
    """A `Correspondence` node over two views, ready to put in a foreground."""
    joiner = Correspondence(left, right)
    joiner.only_selected = only_selected
    return ws.insert_node_dyn(joiner)


def joined_table(ws, left, right, suffix="_2"):
    """The two views' source tables, joined into one, as a pyarrow table.

    Row for row where they came from the same table; on the key columns they
    name otherwise. Columns that collide take `suffix`, so nothing is silently
    dropped — the point of a join is to have both, and a join that quietly threw
    one away would be worse than one that refused.
    """
    import pyarrow as pa

    pairs = row_correspondence(ws, left, right)
    (left, right) = (view_of(ws, left), view_of(ws, right))
    left_table = ws.send_request(left, SourceTable())
    right_table = ws.send_request(right, SourceTable())
    if left_table is None or right_table is None or not pairs:
        return None
    (left_rows, right_rows) = ([a for (a, _) in pairs], [b for (_, b) in pairs])

    columns = {}
    for name in left_table.column_names:
        values = left_table.column(name).to_pylist()
        columns[name] = [values[i] for i in left_rows]
    for name in right_table.column_names:
        values = right_table.column(name).to_pylist()
        key = name if name not in columns else name + suffix
        columns[key] = [values[i] for i in right_rows]
    return pa.table({name: pa.array(values) for (name, values) in columns.items()})


def mirror_selection(ws, source, targets):
    """Push whatever `source` has selected into every one of `targets`.

    The other half of linking: a line shows two marks are the same record, and
    this makes clicking one light up the other. Called from a `tick` or a draw,
    once a frame.
    """
    row = ws.send_request(source, Selection())
    for target in targets:
        ws.send_request(target, SetSelection(row))
    return row


# ======================================================================
# The data explorer
# ======================================================================
#
# Offered in the sidebar as a lambda you wire a table into. Deliberately thin:
# a bar of dropdowns, and the picture is whichever library layout the data
# calls for.
#
# The layout is chosen *from the data*, not picked off a list. One column and a
# mode is all anybody wants to say; which of a scatter, a bar chart, a violin
# and a heatmap that comes to is a question about the columns' types, and the
# answer is never in doubt. So there is no layout dropdown to get wrong.

MODES = ("Univariate", "Bivariate")

CONTROL_H = 30.0
CONTROL_GAP = 8.0
DD_MODE_W = 116.0
DD_COL_W = 148.0
DD_H = 22.0
#: What a column dropdown shows when the table has no columns at all.
NO_COLUMN = "—"

#: How big the picture is drawn inside the explorer, before any magnification.
#: Bigger than the box it is shown in, which is the point: it is panned and
#: zoomed rather than squeezed.
EXPLORER_PLOT_SIZE = (1400.0, 1000.0)


def layout_for(frame, mode, x, y):
    """Which layout a view of these columns in this mode comes to.

    Univariate: a continuous column is a violin of its values — every row a
    point, with the density around them saying what shape the column is — and a
    categorical one is a count of its categories. Bivariate follows the pair — the whole matrix,
    not just the numeric corner of it:

      * continuous x continuous -> a scatter, with the least-squares line;
      * categorical x continuous -> a violin per category;
      * categorical x categorical -> a heatmap of the contingency table.
    """
    if mode == "Univariate" or not y or x == y:
        # A violin rather than a bare strip: the points are still all there, and
        # the density around them says what the shape of the column is, which a
        # column of dots leaves you to guess at.
        return Violin if frame.is_continuous(x) else Bars
    (kx, ky) = (frame.is_continuous(x), frame.is_continuous(y))
    if kx and ky:
        return Scatter
    if kx or ky:
        return Violin
    return Heatmap


def explorer_channels(frame, mode, x, y):
    """`(kind, channels)` for a view of these columns.

    Univariate means *one* column, so the second channel is left unset whatever
    the other dropdown happens to be showing — a violin with a `y` is a violin
    split by `x`, and filling that in from a column nobody chose for the purpose
    turns one distribution into a page of them.

    Bivariate, a violin splits a continuous column *by* a categorical one, so
    whichever way round the two were chosen the category goes on `x` and the
    measure on `y`. Putting that here rather than in the layout means the
    dropdowns stay in the order the reader picked them.
    """
    layout = layout_for(frame, mode, x, y)
    if mode == "Univariate" or not y or x == y:
        return (layout.KIND, {"x": x, "y": None})
    if layout is Violin and frame.is_continuous(x):
        (x, y) = (y, x)
    return (layout.KIND, {"x": x, "y": y})


class DataExplorer:
    """A control bar, and the view it drives, on a plane of its own.

    The picture is a `Plot`, so it can be asked the protocol directly and its
    sensor covers it and nothing else — a sensor takes every click over it, and
    one stretched across the panel would swallow the presses meant for the
    dropdowns.

    **Whether it goes on a plane depends on what it is.** A scatter or a violin
    has a mark per *row*: there is always more of it than fits, and zooming into
    a crowded corner is how you read it. A bar chart or a heatmap has a mark per
    *category* — it is exactly as big as it needs to be, and a plane would only
    add a gesture that does nothing and chrome that floats over a picture which
    was not going anywhere. So the plane is drawn for the first kind, and the
    view is drawn straight into the box for the second, where it draws its own
    axes because there is nowhere else for them to go.

    Changing mode or columns rebuilds none of it. The view *becomes* the layout
    the data calls for, in place — same node, same canvas item, same chrome
    pointing at it — and the explorer simply draws a different one of the two.
    See `Plot.become`.
    """

    def __init__(self, frame, mode_dd, x_dd, y_dd, canvas, plot):
        self.frame = frame
        self.mode_dd = mode_dd
        self.x_dd = x_dd
        self.y_dd = y_dd
        #: The plane the picture is drawn on, and the picture itself.
        self.canvas = canvas
        self.plot = plot

    # -- the node it is --------------------------------------------------

    def owned_nodes(self):
        # Not the plot: the canvas owns it, along with the item holding it and
        # the readout in front of it. Naming it twice would delete it twice.
        return [self.mode_dd, self.x_dd, self.y_dd, self.canvas]

    def on_delete(self, ctx):
        for uid in self.owned_nodes():
            ctx.workspace.delete_node(uid)

    def type_name(self):
        return "A Data Explorer"

    def request(self, req, ctx):
        """The protocol, passed down to the view.

        Another node wires to the explorer, because the explorer is what it can
        see; the view behind it is what knows where the marks went. So the split
        is invisible from outside, and an explorer links to a bare layout as
        readily as two bare layouts link to each other.
        """
        if getattr(req, "name", None) in PROTOCOL:
            return workspace_of(ctx).send_request(self.plot, req)
        return NotImplemented

    # -- drawing ---------------------------------------------------------

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        h = base.y.provided_value() if base.y is not None else None
        if w is None or h is None or not (math.isfinite(w) and math.isfinite(h)):
            return dex.DrawResult.Complete(region=None)
        (x, y) = (base.pos.x, base.pos.y)

        # A ground, but no rule around it: whatever is showing this — a lambda's
        # output slot, a canvas item — has already drawn its own edge, and a
        # second one just inside it reads as a mistake.
        rect(ctx, x, y, w, h, PANEL)
        if not self.frame.columns:
            text(ctx, "no columns to explore", x + MARGIN, y + MARGIN, TITLE_FONT, FAINT)
        else:
            self.read_controls(ctx)
            self.draw_controls(ctx, x, y)
            px = x + MARGIN
            py = y + CONTROL_H + CONTROL_GAP
            pw = w - 2.0 * MARGIN
            ph = h - (CONTROL_H + CONTROL_GAP) - MARGIN
            if pw > 40.0 and ph > 40.0:
                self.draw_view(ctx, px, py, pw, ph)
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))

    def draw_view(self, ctx, x, y, w, h):
        """The picture: on its plane, or straight into the box.

        The view is the same node either way. What changes is who draws the
        axes, the title and the figures — the plane's own chrome, or the view
        itself — so it is told which as it is drawn.
        """
        ws = ctx.node.workspace
        kind = (ws.send_request(self.plot, Encoding()) or {}).get("kind") or ""
        layout = LAYOUTS_BY_KIND.get(kind)
        on_a_plane = bool(layout and layout.WANTS_PLANE)
        ws.send_request(self.plot, SetChrome(not on_a_plane))
        if on_a_plane:
            # The plane, not the plot: drag it to pan, alt-scroll to zoom.
            ctx.draw_node(self.canvas, at(x, y, w, h))
        else:
            ctx.draw_inspectable_node(self.plot, at(x, y, w, h))

    def mode(self, ws):
        index = ws.send_request(self.mode_dd, dex.DropdownSelection())
        return MODES[index] if index is not None and index < len(MODES) else MODES[0]

    def draw_controls(self, ctx, x, y):
        """The dropdowns across the top.

        The second column only has a say in bivariate mode, so its dropdown only
        stands there: a control that does nothing is worse than no control,
        because it invites you to change something and then ignores you.
        """
        cx = x + MARGIN
        cy = y + (CONTROL_H - DD_H) / 2.0
        ctx.draw_node(self.mode_dd, at(cx, cy, DD_MODE_W, DD_H))
        cx += DD_MODE_W + CONTROL_GAP

        text(ctx, "x", cx, y + CONTROL_H / 2.0 - 6.0, TICK_FONT, FAINT)
        cx += 12.0
        ctx.draw_node(self.x_dd, at(cx, cy, DD_COL_W, DD_H))
        cx += DD_COL_W + CONTROL_GAP

        if self.mode(ctx.node.workspace) == "Bivariate":
            text(ctx, "y", cx, y + CONTROL_H / 2.0 - 6.0, TICK_FONT, FAINT)
            cx += 12.0
            ctx.draw_node(self.y_dd, at(cx, cy, DD_COL_W, DD_H))

    def column_of(self, ws, dropdown):
        index = ws.send_request(dropdown, dex.DropdownSelection())
        columns = self.frame.columns
        if index is None or not (0 <= index < len(columns)):
            return columns[0] if columns else None
        return columns[index]

    def read_controls(self, ctx):
        """Make the view agree with the dropdowns, if it does not already.

        Only when something changed: `SetEncoding` rebuilds whatever the layout
        derives from its columns, and a tree rebuilt every frame is a tree
        rebuilt sixty times a second for no reason.
        """
        ws = ctx.node.workspace
        mode = self.mode(ws)
        x = self.column_of(ws, self.x_dd)
        y = self.column_of(ws, self.y_dd)
        if x is None:
            return
        (kind, channels) = explorer_channels(self.frame, mode, x, y)
        current = ws.send_request(self.plot, Encoding()) or {}
        wanted = dict(channels, kind=kind)
        if any(current.get(k) != v for (k, v) in wanted.items()):
            ws.send_request(self.plot, SetEncoding(**wanted))


def build_explorer(ws, source=None, mode=None, x=None, y=None):
    """A `DataExplorer` over `source`, with its picture on a plane of its own.

    `mode`, `x` and `y` open it on a particular view. With nothing wired it
    explores a built-in sample, so it does something the moment it is opened
    rather than asking for a file first.
    """
    frame = Frame(source)
    columns = frame.columns
    mode = mode if mode in MODES else MODES[0]
    x = x if x in columns else (columns[0] if columns else None)
    # A different second column, so bivariate does not open on x against itself.
    y = y if y in columns else (columns[1] if len(columns) > 1 else x)

    mode_dd = dex.Dropdown.build(ws, list(MODES))
    if MODES.index(mode):
        ws.submit_action(mode_dd, dex.SetDropdownSelection(MODES.index(mode)))
    x_dd = dex.Dropdown.build(ws, list(columns) or [NO_COLUMN])
    y_dd = dex.Dropdown.build(ws, list(columns) or [NO_COLUMN])
    if x in columns and columns.index(x):
        ws.submit_action(x_dd, dex.SetDropdownSelection(columns.index(x)))
    if y in columns and columns.index(y):
        ws.submit_action(y_dd, dex.SetDropdownSelection(columns.index(y)))

    (kind, channels) = explorer_channels(frame, mode, x, y)
    view = build_plot(ws, LAYOUTS_BY_KIND[kind], frame=frame, **channels)
    # The readout in the plane's foreground draws the hover card and the row
    # overlay instead of the view, so both stay their own size however far the
    # picture under them is magnified.
    view.chrome = False
    plot = ws.insert_node_dyn(view)
    canvas = on_plane(ws, plot, EXPLORER_PLOT_SIZE, "Data explorer",
                      what="Placed the view")
    adopt(ws, canvas, PlotAxes(canvas, plot), dex.Layer.background())
    adopt(ws, canvas, PlotTitle(canvas, plot), dex.Layer.foreground())
    adopt(ws, canvas, PlotStatsPanel(canvas, plot), dex.Layer.foreground())
    adopt(ws, canvas, PlotChrome(plot), dex.Layer.foreground())
    return DataExplorer(frame, mode_dd, x_dd, y_dd, canvas, plot)


# ======================================================================
# What this workspace offers
# ======================================================================
#
# Two lambdas: one that makes a view of a table, and one that joins two views
# together. Each is registered as a *factory*, not a node — the prelude is run
# again before every lambda, and only a sidebar scan actually calls these. A
# node built here instead would seed a fresh set of dropdowns, sensors and
# canvases on every run.

#: The script the offered lambda arrives holding. Short on purpose — everything
#: it needs is already in scope, because the prelude it runs under is this file.
EXPLORER_SCRIPT = '\n'.join([
    '"""A data explorer over the table wired into `thisData`.',
    '',
    'Everything here comes from the workspace prelude, so this is all of it.',
    'Edit freely: `build_explorer` takes an opening view, and the layouts under',
    'it (`Scatter`, `Violin`, `Heatmap`, `Phylogeny`, `Circos`, `Scatter3D`) can',
    'be built directly with `build_plot` if you want one it would not choose.',
    '"""',
    '',
    '',
    'def transform():',
    '    return build_explorer(dex.ws, thisData)',
    '',
])

#: The argument the lambda declares: what a table is wired into, and what the
#: row calls it. Together they read "for: thisData (a table)" — the label says
#: what wiring something in is *for*, which a parameter name on its own does not.
EXPLORER_ARG_LABEL = "for"
EXPLORER_ARG = "thisData"

#: How big the lambda is placed. Room for the control bar, the plane under it,
#: and the script above both.
EXPLORER_SIZE = (600.0, 700.0)


def build_explorer_lambda(ws):
    """The offered node: a lambda that generates a data explorer from a table.

    A lambda rather than the explorer itself, because a view of a table has to
    be *given* the table, and being wired into is how a node is given anything
    here. It also means what you place stays yours: the script is right there,
    and everything the prelude defines is in scope inside it.

    The argument is declared `a table`, so the port says what belongs in it
    before anything is wired, and complains if the wrong thing is.
    """
    # The ids the caller has to know before the queue drains: the argument row
    # to add to, and the output anything downstream wires to.
    args = dex.NodeUid.mint()
    output = dex.NodeUid.mint()
    lam = dex.Lambda.new_with(
        ws, args, output, "Generate data explorer", EXPLORER_SCRIPT)

    arg = dex.NodeUid.mint()
    port = dex.NodeUid.mint()
    dex.LambdaArg.build_with(ws, arg, port, EXPLORER_ARG_LABEL, EXPLORER_ARG)
    ws.submit_action(args, dex.AddArgAt(arg), "Added the table argument")
    ws.submit_action(arg, dex.SetArgKind(dex.ArgType.Table, ""), "Declared it a table")
    return lam


dex.prelude_prototypes.add(
    "Generate data explorer", build_explorer_lambda, EXPLORER_SIZE)
