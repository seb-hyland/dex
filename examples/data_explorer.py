"""A data explorer for a table: pick a column and see its shape, pick two and
see how they relate.

Wire a `Table` into a lambda argument named `thisData` and this builds one
node — "Generate data explorer / for: thisData (a table)" — that holds the table
and draws it. With nothing wired it explores a small built-in sample, so it does
something the moment it is opened.

It has two modes, chosen from a dropdown at the top:

  * **Univariate.** One column. A *continuous* column is a vertical strip plot —
    every row a dot at its value, jittered sideways so a stack of equal values
    fans out instead of hiding behind itself. A *categorical* column is a bar
    chart of how often each category occurs.
  * **Bivariate.** Two columns, and the plot follows their types — the whole
    matrix, not just the numeric corner of it:
      - continuous × continuous → a scatter, with the least-squares line and
        Pearson's r;
      - categorical × continuous → a strip per category with its quartile box,
        and each group's mean and n;
      - categorical × categorical → a heatmap of the contingency table, with the
        totals and Cramér's V.

**Every mark knows its row.** Hover a point and a readout names the record it
came from; click it and a panel overlays the whole row. There is no node per
point behind that — a table of any size would grind to a halt if there were, one
inspectable node redrawn and hit-tested every frame. Instead one sensor covers
the plot, and a click is answered by searching what was drawn for the nearest
mark, the way `pdb_viewer.py` picks an atom. Click empty space to dismiss.

That search is the same machinery that lets *another* visualization ask this one
where a given row is drawn. Every plot here answers the query protocol in the
workspace prelude — `RowKeys`, `DrawnPoints`, `DrawnPoint`, `RowValues` — so a
second view built from the same table can wire to this node and, each frame, ask
where row 7 landed and draw a line to wherever *it* drew row 7. Two pictures of
one table, joined record by record, without either knowing what the other is.

Type detection is by Arrow dtype and cardinality: text and booleans are
categorical, numbers are continuous — unless a numeric column has only a handful
of distinct values (a 1/2/3 rating, a 0/1 flag), which is a category wearing a
number's clothes and is treated as one.

**The picture is its own node.** `DataExplorer` is the surface — a page, a bar of
dropdowns — and the plot inside it is a separate `Plot`, drawn with
`draw_inspectable_node` into the box the controls leave. So the plot is
addressable: click it and the inspector opens the *view's* panel, headed by what
it currently is ("A Scatter of body_mass_g x flipper_mm"), not the explorer's.

That split is also what makes the dropdowns work. A sensor takes every click
over it, and while the plot was drawn inline its sensor stretched across the
whole panel, control bar included — so a press on a dropdown never reached the
dropdown. A sensor belonging to the plot can only cover the plot.

Nothing is pushed between the two: the plot reads the dropdowns itself each
frame, so a choice changes the picture on the next frame with no action in
between, and the undo history records the choice and nothing else. The mode, the
chosen columns and the row being hovered are all the plot's own state, so a
clone carries a whole, consistent explorer rather than halves that have drifted
apart. The columns are stored as plain Python lists, which keeps the node
picklable, the rule every example here follows.

Reading the table needs nothing installed; *building* the sample needs nothing
either. Only a real wired `Table` arrives as a pyarrow table, and reading one is
just `.column(name).to_pylist()`.
"""

import math
import random

# ======================================================================
# The query protocol's tags
# ======================================================================
#
# The message *classes* live in the workspace prelude (see `prelude.py`); these
# are the string tags they carry, and the whole of the contract a plot needs in
# order to *answer*. A plot that only answers questions need not know the
# classes — just these tags — so it works before any querying view exists. Keep
# them identical to the prelude's.
ROW_KEYS = "dex.plot.row_keys"
DRAWN_POINTS = "dex.plot.drawn_points"
DRAWN_POINT = "dex.plot.drawn_point"
ROW_VALUES = "dex.plot.row_values"
# Every tag the surface hands down to the plot behind it.
PROTOCOL = (ROW_KEYS, DRAWN_POINTS, DRAWN_POINT, ROW_VALUES)

# ======================================================================
# Look
# ======================================================================

MODES = ("Univariate", "Bivariate")

# Stored as plain RGB triples, not `dex.Color`s: the palette is data, and
# building the colour at draw time keeps the node picklable.
INK = (44, 49, 58)
FAINT = (122, 128, 138)
AXIS = (150, 156, 166)
GRID = (232, 234, 240)
PANEL = (255, 255, 255)
PANEL_EDGE = (208, 213, 222)
POINT = (72, 130, 220)
LINE = (226, 110, 92)

# One colour per category, cycled. Chosen to stay apart in both hue and value.
CATEGORY_PALETTE = [
    (72, 130, 220), (226, 110, 92), (96, 176, 118), (176, 132, 210),
    (222, 168, 70), (86, 188, 196), (210, 120, 160), (140, 150, 160),
]

# The bar/cell for a category, and the sequential ramp a heatmap counts along.
HEAT_LOW = (240, 243, 248)
HEAT_HIGH = (36, 92, 170)

POINT_R = 4.0
HOVER_REACH = 12.0          # how close the pointer must come, in pixels
BAR_FONT = 11.0
TICK_FONT = 10.0
TITLE_FONT = 13.0
READOUT_FONT = 11.0
STAT_FONT = 11.0

CONTROL_H = 30.0            # the control bar across the top
CONTROL_GAP = 8.0
DD_MODE_W = 116.0
DD_COL_W = 150.0
DD_H = 22.0                 # the nominal row height a dropdown is given

MARGIN = 14.0
GUTTER = 52.0              # left column for the value axis
FOOT = 34.0               # bottom band for the category / value axis
TOP = 26.0                # room for the title over the plot

# A numeric column with no more distinct values than this is really a category.
CATEGORICAL_MAX_LEVELS = 12
# How many categories a bar/heatmap axis will draw before it stops.
MAX_CATEGORIES = 40


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
    sx = sum(x for x, _ in pairs)
    sy = sum(y for _, y in pairs)
    mx, my = sx / n, sy / n
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
    sx = sum(x for x, _ in pairs)
    sy = sum(y for _, y in pairs)
    mx, my = sx / n, sy / n
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


def nice_bounds(lo, hi, target=5):
    """A rounded `(lo, hi, step)` covering `[lo, hi]` in ~`target` steps."""
    if not math.isfinite(lo) or not math.isfinite(hi):
        return (0.0, 1.0, 0.5)
    if hi <= lo:
        pad = abs(lo) * 0.5 or 1.0
        lo, hi = lo - pad, hi + pad
    raw = (hi - lo) / max(target, 1)
    mag = 10.0 ** math.floor(math.log10(raw)) if raw > 0 else 1.0
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
    text = f"{value:.{places}f}"
    return "0" if text in ("-0", "-0.0", "-0.00") else text


# ======================================================================
# Reading the table
# ======================================================================


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


def classify(name, values, is_numeric):
    """Whether a column is `"continuous"` or `"categorical"`.

    Text and booleans are categorical. Numbers are continuous, unless they take
    only a few distinct values — a rating or a flag — in which case they are a
    category that happens to be written as a number.
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


def read_table(source):
    """`(columns, data, kinds)` from whatever `source` is.

    A wired `Table` arrives as a pyarrow table; a dict of lists is taken as is;
    and nothing at all falls back to the sample, so the explorer opens onto
    something. `data` maps a column name to its list of values; `kinds` maps it
    to `"continuous"` or `"categorical"`.
    """
    if source is None:
        source = sample_table()

    if isinstance(source, dict):
        columns = list(source.keys())
        data = {c: list(source[c]) for c in columns}
        kinds = {
            c: classify(c, data[c], all(
                isinstance(v, (int, float)) and not isinstance(v, bool)
                for v in data[c] if v is not None
            ) and any(v is not None for v in data[c]))
            for c in columns
        }
        return columns, data, kinds

    # A pyarrow table: read each column and read its declared type.
    import pyarrow.types as pat

    columns = list(source.column_names)
    data = {}
    kinds = {}
    for c in columns:
        col = source.column(c)
        data[c] = col.to_pylist()
        t = col.type
        is_numeric = (pat.is_integer(t) or pat.is_floating(t)) and not pat.is_boolean(t)
        kinds[c] = classify(c, data[c], is_numeric)
    return columns, data, kinds


def _show(v):
    """A value as it should read in a readout."""
    if isinstance(v, float):
        return f"{v:.4g}"
    if v is None:
        return "—"
    return str(v)


# ======================================================================
# Painting
# ======================================================================
#
# Free functions, not methods: both nodes below paint, and the ink does not
# belong to either of them.


def _abs():
    return dex.DrawConstraints(
        pos=dex.ScreenPos.new(0.0, 0.0), x=None, y=None, wrap=None, should_clip=False
    )


def _at(x, y, w=None, h=None):
    return dex.DrawConstraints(
        pos=dex.ScreenPos.new(x, y),
        x=dex.AxisConstraint.Exactly(w) if w is not None else None,
        y=dex.AxisConstraint.Exactly(h) if h is not None else None,
        wrap=None, should_clip=False,
    )


def _line(ctx, pts, rgb, w=1.0):
    ctx.draw_node(
        dex.Path.polyline([dex.Vector.new(x, y) for (x, y) in pts],
                          dex.Stroke.new(w, dex.Color.rgb(*rgb))),
        _abs(),
    )


def _dot(ctx, cx, cy, r, rgb):
    ctx.draw_node(dex.Circle.new(r, dex.Color.rgb(*rgb)), _at(cx - r, cy - r))


def _text(ctx, s, x, y, size, rgb, bold=False):
    lab = dex.Label.new(s)
    font = dex.Font.proportional(size)
    font.bold = bold
    lab.font = font
    lab.color = dex.Color.rgb(*rgb)
    ctx.draw_node(lab, _at(x, y))


def _measure(ctx, s, size, bold=False):
    font = dex.Font.proportional(size)
    font.bold = bold
    m = ctx.measure_text(s, font, dex.TextWrap.singleline())
    return (m.width, m.height)


def _rect(ctx, x, y, w, h, rgb, edge=None):
    if edge is None:
        ctx.draw_node(dex.Rect.new(w, h, dex.Color.rgb(*rgb)), _at(x, y, w, h))
    else:
        ctx.draw_node(
            dex.Rect.bordered(w, h, dex.Color.rgb(*rgb), 0.0,
                              dex.Stroke.new(1.0, dex.Color.rgb(*edge))),
            _at(x, y, w, h),
        )


# ======================================================================
# The explorer
# ======================================================================


class Plot:
    """The picture itself: a node of its own, holding the table and the marks.

    It is drawn into whatever box the explorer allots it — the box is the
    geometry, so it needs no stored anchors — and it carries the single sensor
    that answers hover and click. That sensor covers the plot and nothing else,
    which is the whole reason this is a separate node: a sensor stretched over
    the surrounding panel would take the clicks meant for the dropdowns above
    it before they ever arrived.

    It reads the controls rather than being told about them, so choosing an
    option changes the picture on the next frame with no action in between —
    and nothing extra in the undo history beside the choice itself.

    What it drew, and where, it records in `_frame_pos`, which is what the hover
    search, the click selection and the query protocol all read.
    """

    def __init__(self, columns, data, kinds, mode_dd, x_dd, y_dd, sensor):
        self.columns = list(columns)
        self.data = data
        self.kinds = kinds
        self.n = len(data[columns[0]]) if columns else 0
        # The controls, read each frame. Owned by the explorer, not by the plot.
        self.mode_dd = mode_dd
        self.x_dd = x_dd
        self.y_dd = y_dd
        self.sensor = sensor
        # State mirrored from the dropdowns, so a first frame has an answer.
        self.mode = MODES[0]
        self.x_col = columns[0] if columns else None
        self.y_col = columns[1] if len(columns) > 1 else (columns[0] if columns else None)
        # The row a click selected, shown in the overlay, or None.
        self.selected = None
        # A stable sideways jitter per row, so a strip plot does not shimmer.
        self._jit = [random.Random(i * 2654435761).uniform(-1.0, 1.0)
                     for i in range(self.n)]
        # Rebuilt every frame: {row_id: (screen_x, screen_y)}.
        self._frame_pos = {}
        # The overlay's screen rect last frame, so a click on it does not
        # dismiss it by "hitting empty space" underneath.
        self._overlay_rect = None

    # -- persistence -----------------------------------------------------

    def __getstate__(self):
        """Everything but the per-frame positions: they belong to a screen and a
        frame, not to the plot."""
        state = self.__dict__.copy()
        state["_frame_pos"] = {}
        state["_overlay_rect"] = None
        return state

    def owned_nodes(self):
        return [self.sensor]

    def on_delete(self, ctx):
        ctx.workspace.delete_node(self.sensor)

    # -- what this plot is -----------------------------------------------

    def _chart(self):
        """The kind of picture the current view comes out as."""
        kx = self.kinds.get(self.x_col)
        if self.mode == "Univariate":
            return "A Strip Plot" if kx == "continuous" else "A Bar Chart"
        if self.x_col == self.y_col:
            return "A Plot"
        ky = self.kinds.get(self.y_col)
        if kx == "continuous" and ky == "continuous":
            return "A Scatter"
        if "continuous" in (kx, ky):
            return "A Strip Plot by Category"
        return "A Heatmap"

    def type_name(self):
        """Named for what it is showing, so the inspector's header says so."""
        if not self.columns:
            return "An Empty Plot"
        if self.mode == "Univariate":
            return "%s of %s" % (self._chart(), self.x_col)
        return "%s of %s x %s" % (self._chart(), self.x_col, self.y_col)

    def build_inspector(self, ctx):
        """The view's own panel, opened by clicking the plot.

        Returned as a value rather than a uid: the workspace gives it a home
        and seats it under `type_name`, so nothing has to be built by hand.
        """
        rows = ["Mode: %s" % self.mode, "Rows: %d" % self.n]
        shown = [self.x_col] if self.mode == "Univariate" else [self.x_col, self.y_col]
        for axis, col in zip(("x", "y"), shown):
            rows.append("%s: %s (%s)" % (axis, col, self.kinds.get(col, "unknown")))
        if self.selected is not None and 0 <= self.selected < self.n:
            rows.append("")
            rows.append("Selected row %d" % self.selected)
            rows += ["    %s: %s" % (c, _show(self.data[c][self.selected]))
                     for c in self.columns]
        return dex.Label.new("\n".join(rows))

    # -- the query protocol ----------------------------------------------

    def request(self, req, ctx):
        """Answer any view that speaks the prelude's protocol. Matched by the
        message's `name` tag, never its class, so it survives save and clone."""
        tag = getattr(req, "name", None)
        if tag == ROW_KEYS:
            return list(range(self.n))
        if tag == DRAWN_POINTS:
            return dict(self._frame_pos)
        if tag == DRAWN_POINT:
            return self._frame_pos.get(req.row_id)
        if tag == ROW_VALUES:
            if 0 <= req.row_id < self.n:
                return {c: self.data[c][req.row_id] for c in self.columns}
            return None
        return NotImplemented

    # -- drawing ---------------------------------------------------------

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        h = base.y.provided_value() if base.y is not None else None
        if w is None or h is None or not (math.isfinite(w) and math.isfinite(h)):
            return dex.DrawResult.Complete(region=None)
        (px, py) = (base.pos.x, base.pos.y)
        self._frame_pos = {}
        if not self.columns:
            return self._done(base, w, h)

        ws = ctx.node.workspace
        self._read_controls(ws)
        if w > 40.0 and h > 40.0:
            if self.mode == "Univariate":
                self._draw_univariate(ctx, px, py, w, h)
            else:
                self._draw_bivariate(ctx, px, py, w, h)

        # Hover, click and the detail overlay, over everything they name.
        self._interact(ctx, ws, px, py, w, h)
        return self._done(base, w, h)

    def _done(self, base, w, h):
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))

    def _read_controls(self, ws):
        """Mirror the dropdowns onto this frame's view."""
        mode_idx = ws.send_request(self.mode_dd, dex.DropdownSelection())
        if mode_idx is not None and 0 <= mode_idx < len(MODES):
            self.mode = MODES[mode_idx]
        x_idx = ws.send_request(self.x_dd, dex.DropdownSelection())
        if x_idx is not None and 0 <= x_idx < len(self.columns):
            self.x_col = self.columns[x_idx]
        y_idx = ws.send_request(self.y_dd, dex.DropdownSelection())
        if y_idx is not None and 0 <= y_idx < len(self.columns):
            self.y_col = self.columns[y_idx]

    # -- univariate ------------------------------------------------------

    def _draw_univariate(self, ctx, px, py, pw, ph):
        col = self.x_col
        if self.kinds.get(col) == "continuous":
            self._strip_one(ctx, px, py, pw, ph, col)
        else:
            self._bars(ctx, px, py, pw, ph, col)

    def _value_axis(self, ctx, px, py, pw, ph, lo, hi, step, title):
        """A vertical value axis down the left gutter, and horizontal gridlines.
        Returns a `to_y(value)` mapping into the plot band."""
        top = py + TOP
        bottom = py + ph - FOOT
        span = hi - lo if hi > lo else 1.0

        def to_y(v):
            return bottom - (v - lo) / span * (bottom - top)

        for t in axis_ticks(lo, hi, step):
            gy = to_y(t)
            _line(ctx, [(px + GUTTER, gy), (px + pw, gy)], GRID, 1.0)
            cap = tick_text(t, step)
            (cw, chh) = _measure(ctx, cap, TICK_FONT)
            _text(ctx, cap, px + GUTTER - 6.0 - cw, gy - chh / 2.0, TICK_FONT, FAINT)
        _line(ctx, [(px + GUTTER, top), (px + GUTTER, bottom)], AXIS, 1.2)
        _text(ctx, title, px + GUTTER, py, TITLE_FONT, INK, bold=True)
        return to_y, top, bottom

    def _strip_one(self, ctx, px, py, pw, ph, col):
        """A vertical strip plot of one continuous column."""
        idx = [i for i in range(self.n)
               if isinstance(self.data[col][i], (int, float))
               and not isinstance(self.data[col][i], bool)
               and math.isfinite(self.data[col][i])]
        if not idx:
            _text(ctx, "nothing numeric to plot", px + GUTTER, py + TOP, BAR_FONT, FAINT)
            return
        vals = [float(self.data[col][i]) for i in idx]
        lo, hi, step = nice_bounds(min(vals), max(vals))
        to_y, _, _ = self._value_axis(ctx, px, py, pw, ph, lo, hi, step, col)

        band_x = px + GUTTER + (pw - GUTTER) / 2.0
        spread = min(60.0, (pw - GUTTER) / 3.0)
        for i in idx:
            cx = band_x + self._jit[i] * spread
            cy = to_y(float(self.data[col][i]))
            _dot(ctx, cx, cy, POINT_R, POINT)
            self._record(i, cx, cy)

    def _bars(self, ctx, px, py, pw, ph, col):
        """A bar chart of how often each category of `col` occurs."""
        levels, counts, per_row = self._tally(col)
        if not levels:
            _text(ctx, "no categories to count", px + GUTTER, py + TOP, BAR_FONT, FAINT)
            return
        hi = max(counts)
        top_lo, top_hi, step = nice_bounds(0, hi)
        top_lo = 0.0
        to_y, top, bottom = self._value_axis(ctx, px, py, pw, ph, top_lo, top_hi, step,
                                             "count of %s" % col)

        n = len(levels)
        slot = (pw - GUTTER) / n
        bw = min(slot * 0.7, 80.0)
        for j, level in enumerate(levels):
            cx = px + GUTTER + slot * (j + 0.5)
            bar_top = to_y(counts[j])
            rgb = CATEGORY_PALETTE[j % len(CATEGORY_PALETTE)]
            _rect(ctx, cx - bw / 2.0, bar_top, bw, bottom - bar_top, rgb)
            self._x_caption(ctx, level, cx, bottom, slot)
            _text(ctx, str(counts[j]),
                       cx - _measure(ctx, str(counts[j]), TICK_FONT)[0] / 2.0,
                       bar_top - 14.0, TICK_FONT, INK)
            # Every row of this category is located at the bar's top-centre, so a
            # link from another view still lands on it.
            for i in per_row[level]:
                self._record(i, cx, bar_top)

    # -- bivariate -------------------------------------------------------

    def _draw_bivariate(self, ctx, px, py, pw, ph):
        kx = self.kinds.get(self.x_col)
        ky = self.kinds.get(self.y_col)
        if self.x_col == self.y_col:
            _text(ctx, "pick two different columns", px + GUTTER, py + TOP,
                       BAR_FONT, FAINT)
        elif kx == "continuous" and ky == "continuous":
            self._scatter(ctx, px, py, pw, ph)
        elif kx == "categorical" and ky == "continuous":
            self._strip_by(ctx, px, py, pw, ph, self.x_col, self.y_col)
        elif kx == "continuous" and ky == "categorical":
            self._strip_by(ctx, px, py, pw, ph, self.y_col, self.x_col, swapped=True)
        else:
            self._heatmap(ctx, px, py, pw, ph, self.x_col, self.y_col)

    def _scatter(self, ctx, px, py, pw, ph):
        cx_col, cy_col = self.x_col, self.y_col
        idx = [i for i in range(self.n)
               if _is_num(self.data[cx_col][i]) and _is_num(self.data[cy_col][i])]
        if not idx:
            _text(ctx, "nothing numeric to plot", px + GUTTER, py + TOP, BAR_FONT, FAINT)
            return
        xs = [float(self.data[cx_col][i]) for i in idx]
        ys = [float(self.data[cy_col][i]) for i in idx]
        xlo, xhi, xstep = nice_bounds(min(xs), max(xs))
        ylo, yhi, ystep = nice_bounds(min(ys), max(ys))
        to_y, top, bottom = self._value_axis(ctx, px, py, pw, ph, ylo, yhi, ystep, cy_col)
        left = px + GUTTER
        right = px + pw
        xspan = xhi - xlo if xhi > xlo else 1.0

        def to_x(v):
            return left + (v - xlo) / xspan * (right - left)

        for t in axis_ticks(xlo, xhi, xstep):
            cap = tick_text(t, xstep)
            _text(ctx, cap, to_x(t) - _measure(ctx, cap, TICK_FONT)[0] / 2.0,
                       bottom + 6.0, TICK_FONT, FAINT)
        _text(ctx, cx_col,
                   right - _measure(ctx, cx_col, TICK_FONT)[0],
                   bottom + 18.0, TICK_FONT, INK)

        pairs = list(zip(xs, ys))
        fit = least_squares(pairs)
        if fit is not None:
            (a, b) = fit
            _line(ctx, [(to_x(xlo), to_y(a * xlo + b)),
                             (to_x(xhi), to_y(a * xhi + b))], LINE, 1.6)

        for k, i in enumerate(idx):
            cx = to_x(xs[k])
            cy = to_y(ys[k])
            _dot(ctx, cx, cy, POINT_R, POINT)
            self._record(i, cx, cy)

        r = pearson(pairs)
        stats = ["n = %d" % len(idx)]
        if r is not None:
            stats.append("r = %+.3f" % r)
        if fit is not None:
            stats.append("slope = %.3g" % fit[0])
        self._stats(ctx, px, py, pw, stats)

    def _strip_by(self, ctx, px, py, pw, ph, cat_col, num_col, swapped=False):
        """A strip plot of `num_col` split by the categories of `cat_col`, with a
        quartile box and the mean over each group."""
        levels, _, per_row = self._tally(cat_col)
        groups = []
        for level in levels:
            vals = [(i, float(self.data[num_col][i])) for i in per_row[level]
                    if _is_num(self.data[num_col][i])]
            if vals:
                groups.append((level, vals))
        if not groups:
            _text(ctx, "nothing numeric to plot", px + GUTTER, py + TOP, BAR_FONT, FAINT)
            return
        allv = [v for _, vals in groups for _, v in vals]
        lo, hi, step = nice_bounds(min(allv), max(allv))
        to_y, top, bottom = self._value_axis(ctx, px, py, pw, ph, lo, hi, step, num_col)

        n = len(groups)
        slot = (pw - GUTTER) / n
        half = min(slot * 0.3, 34.0)
        for j, (level, vals) in enumerate(groups):
            cx = px + GUTTER + slot * (j + 0.5)
            rgb = CATEGORY_PALETTE[j % len(CATEGORY_PALETTE)]
            q1, med, q3 = quartiles([v for _, v in vals])
            # The quartile box behind the points.
            _rect(ctx, cx - half, to_y(q3), 2.0 * half, to_y(q1) - to_y(q3),
                       _tint(rgb))
            _line(ctx, [(cx - half, to_y(med)), (cx + half, to_y(med))], rgb, 1.6)
            for i, v in vals:
                jx = cx + self._jit[i] * half * 0.9
                cy = to_y(v)
                _dot(ctx, jx, cy, POINT_R, rgb)
                self._record(i, jx, cy)
            self._x_caption(ctx, level, cx, bottom, slot)
            mean = sum(v for _, v in vals) / len(vals)
            _text(ctx, "μ %.4g" % mean,
                       cx - _measure(ctx, "μ %.4g" % mean, TICK_FONT)[0] / 2.0,
                       to_y(hi) - 2.0, TICK_FONT, INK)

        self._stats(ctx, px, py, pw,
                    ["%d groups" % len(groups), "n = %d" % len(allv)])

    def _heatmap(self, ctx, px, py, pw, ph, xcol, ycol):
        """A heatmap of the contingency table of two categorical columns."""
        xlevels = self._levels(xcol)
        ylevels = self._levels(ycol)
        if not xlevels or not ylevels:
            _text(ctx, "no categories to cross", px + GUTTER, py + TOP, BAR_FONT, FAINT)
            return
        xi = {v: j for j, v in enumerate(xlevels)}
        yi = {v: j for j, v in enumerate(ylevels)}
        counts = [[0] * len(xlevels) for _ in ylevels]
        cell_rows = {}
        total = 0
        for i in range(self.n):
            xv, yv = self.data[xcol][i], self.data[ycol][i]
            if xv in xi and yv in yi:
                counts[yi[yv]][xi[xv]] += 1
                cell_rows.setdefault((yi[yv], xi[xv]), []).append(i)
                total += 1
        peak = max((c for row in counts for c in row), default=0) or 1

        grid_x = px + GUTTER
        top = py + TOP
        bottom = py + ph - FOOT
        cell_w = (px + pw - grid_x) / len(xlevels)
        cell_h = (bottom - top) / len(ylevels)
        _text(ctx, "%s × %s" % (xcol, ycol), grid_x, py, TITLE_FONT, INK, bold=True)

        for r, ylevel in enumerate(ylevels):
            cy0 = top + r * cell_h
            (lw, lh) = _measure(ctx, str(ylevel), TICK_FONT)
            _text(ctx, str(ylevel), grid_x - 6.0 - lw, cy0 + cell_h / 2.0 - lh / 2.0,
                       TICK_FONT, FAINT)
            for c, xlevel in enumerate(xlevels):
                cx0 = grid_x + c * cell_w
                count = counts[r][c]
                shade = _lerp(HEAT_LOW, HEAT_HIGH, count / peak)
                _rect(ctx, cx0 + 1.0, cy0 + 1.0, cell_w - 2.0, cell_h - 2.0, shade)
                if count and cell_w > 22.0 and cell_h > 16.0:
                    cap = str(count)
                    (cw, chh) = _measure(ctx, cap, TICK_FONT)
                    ink = PANEL if count / peak > 0.55 else INK
                    _text(ctx, cap, cx0 + cell_w / 2.0 - cw / 2.0,
                               cy0 + cell_h / 2.0 - chh / 2.0, TICK_FONT, ink)
                centre = (cx0 + cell_w / 2.0, cy0 + cell_h / 2.0)
                for i in cell_rows.get((r, c), []):
                    self._record(i, centre[0], centre[1])

        for c, xlevel in enumerate(xlevels):
            cx0 = grid_x + c * cell_w
            self._x_caption(ctx, xlevel, cx0 + cell_w / 2.0, bottom, cell_w)

        v = cramers_v(counts, len(ylevels), len(xlevels), total)
        stats = ["n = %d" % total]
        if v is not None:
            stats.append("Cramér's V = %.3f" % v)
        self._stats(ctx, px, py, pw, stats)

    # -- shared pieces ---------------------------------------------------

    def _levels(self, col):
        """The distinct categories of `col`, first-seen order, capped."""
        out = []
        for v in self.data[col]:
            if v is not None and v not in out:
                out.append(v)
                if len(out) >= MAX_CATEGORIES:
                    break
        return out

    def _tally(self, col):
        """`(levels, counts, rows_by_level)` for a categorical column."""
        levels = self._levels(col)
        rank = {v: j for j, v in enumerate(levels)}
        counts = [0] * len(levels)
        rows = {v: [] for v in levels}
        for i, v in enumerate(self.data[col]):
            if v in rank:
                counts[rank[v]] += 1
                rows[v].append(i)
        return levels, counts, rows

    def _x_caption(self, ctx, text, cx, baseline, slot):
        """A category caption under the axis, truncated to its slot."""
        text = str(text)
        while text and _measure(ctx, text, TICK_FONT)[0] > slot - 4.0 and len(text) > 1:
            text = text[:-1]
        (cw, _) = _measure(ctx, text, TICK_FONT)
        _text(ctx, text, cx - cw / 2.0, baseline + 6.0, TICK_FONT, INK)

    def _stats(self, ctx, px, py, pw, lines):
        """A small stats panel pinned to the plot's top-right."""
        if not lines:
            return
        width = max(_measure(ctx, s, STAT_FONT)[0] for s in lines) + 16.0
        height = len(lines) * (STAT_FONT + 5.0) + 8.0
        x = px + pw - width - 4.0
        y = py + TOP
        ctx.draw_node(
            dex.Rect.bordered(width, height, dex.Color.rgba(255, 255, 255, 235), 4.0,
                              dex.Stroke.new(1.0, dex.Color.rgb(*PANEL_EDGE))),
            _at(x, y, width, height),
        )
        for k, s in enumerate(lines):
            _text(ctx, s, x + 8.0, y + 5.0 + k * (STAT_FONT + 5.0), STAT_FONT, INK)

    def _record(self, i, cx, cy):
        """Note where row `i` was drawn this frame — all the picking there is."""
        self._frame_pos[i] = (cx, cy)

    # -- hover, click and the overlay ------------------------------------

    def _nearest(self, pointer):
        """The row whose mark is nearest the pointer, within reach, or None."""
        best = None
        for i, (cx, cy) in self._frame_pos.items():
            gap = (cx - pointer.x) ** 2 + (cy - pointer.y) ** 2
            if gap <= HOVER_REACH ** 2 and (best is None or gap < best[0]):
                best = (gap, i)
        return None if best is None else best[1]

    def _interact(self, ctx, ws, ox, oy, w, h):
        """One sensor over the plot: hover names the nearest mark, a click
        selects it (or clears the selection on empty space)."""
        ctx.draw_node(
            self.sensor,
            dex.DrawConstraints(
                pos=dex.ScreenPos.new(ox, oy),
                x=dex.AxisConstraint.Exactly(w),
                y=dex.AxisConstraint.Exactly(h),
                wrap=None, should_clip=False,
            ),
        )
        pointer = ws.send_request(self.sensor, dex.PointerPos())
        hovered = self._nearest(pointer) if pointer is not None else None

        if ws.send_request(self.sensor, dex.TakeClicked()) and pointer is not None:
            if self._overlay_rect and _inside(pointer, self._overlay_rect):
                pass  # a click on the card is not a click on the plot
            elif hovered is not None:
                self.selected = hovered
            else:
                self.selected = None

        if hovered is not None:
            self._hover_readout(ctx, ox, oy, w, h, hovered)
        self._overlay_rect = None
        if self.selected is not None and 0 <= self.selected < self.n:
            self._detail_overlay(ctx, ox, oy, w, h, self.selected)

    def _hover_readout(self, ctx, ox, oy, w, h, i):
        """A small card by the mark, naming the row and its plotted columns."""
        (cx, cy) = self._frame_pos[i]
        ctx.draw_node(
            dex.Circle.bordered(POINT_R + 4.0, dex.Color.transparent(),
                                dex.Stroke.new(1.5, dex.Color.rgb(*INK))),
            _at(cx - POINT_R - 4.0, cy - POINT_R - 4.0),
        )
        cols = [self.x_col] if self.mode == "Univariate" else [self.x_col, self.y_col]
        lines = ["row %d" % i] + ["%s: %s" % (c, _show(self.data[c][i])) for c in cols]
        cap_w = max(_measure(ctx, s, READOUT_FONT)[0] for s in lines) + 14.0
        cap_h = len(lines) * (READOUT_FONT + 4.0) + 8.0
        left = min(cx + 10.0, ox + w - cap_w - 4.0)
        topp = min(max(cy - cap_h - 8.0, oy + 4.0), oy + h - cap_h - 4.0)
        ctx.draw_node(
            dex.Rect.bordered(cap_w, cap_h, dex.Color.rgba(255, 255, 255, 244), 4.0,
                              dex.Stroke.new(1.0, dex.Color.rgb(*PANEL_EDGE))),
            _at(left, topp, cap_w, cap_h),
        )
        for k, s in enumerate(lines):
            _text(ctx, s, left + 7.0, topp + 5.0 + k * (READOUT_FONT + 4.0),
                       READOUT_FONT, INK, bold=(k == 0))

    def _detail_overlay(self, ctx, ox, oy, w, h, i):
        """The whole row, pinned to the bottom-right, over everything.

        A ring marks the selected mark wherever it is; the card lists every
        column. Clicking empty space clears it (see `_interact`).
        """
        pos = self._frame_pos.get(i)
        if pos is not None:
            ctx.draw_node(
                dex.Circle.bordered(POINT_R + 6.0, dex.Color.transparent(),
                                    dex.Stroke.new(2.0, dex.Color.rgb(*LINE))),
                _at(pos[0] - POINT_R - 6.0, pos[1] - POINT_R - 6.0),
            )
        lines = ["Row %d" % i] + ["%s: %s" % (c, _show(self.data[c][i]))
                                   for c in self.columns]
        row_h = READOUT_FONT + 5.0
        cap_w = max(_measure(ctx, s, READOUT_FONT)[0] for s in lines) + 18.0
        cap_h = len(lines) * row_h + 12.0
        left = ox + w - cap_w - 6.0
        top = oy + h - cap_h - 6.0
        self._overlay_rect = (left, top, cap_w, cap_h)
        ctx.draw_node(
            dex.Rect.bordered(cap_w, cap_h, dex.Color.rgba(255, 255, 255, 250), 5.0,
                              dex.Stroke.new(1.0, dex.Color.rgb(*PANEL_EDGE))),
            _at(left, top, cap_w, cap_h),
        )
        for k, s in enumerate(lines):
            _text(ctx, s, left + 9.0, top + 6.0 + k * row_h, READOUT_FONT, INK,
                       bold=(k == 0))


def _is_num(v):
    return isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v)


def _tint(rgb, toward=(255, 255, 255), t=0.72):
    return tuple(int(round(rgb[i] + (toward[i] - rgb[i]) * t)) for i in range(3))


def _lerp(a, b, t):
    t = max(0.0, min(1.0, t))
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def _inside(pointer, rect):
    (x, y, w, h) = rect
    return x <= pointer.x <= x + w and y <= pointer.y <= y + h


# ======================================================================
# The surface
# ======================================================================


class DataExplorer:
    """The control bar, and the plot it drives.

    Two nodes, not one. The picture is a `Plot` of its own, drawn with
    `draw_inspectable_node` into the box left under the controls, so it is an
    addressable element: clicking it opens *its* panel, named for the view it
    is currently showing, rather than the surface's. The box is both its
    geometry and its probe region, so nothing about it has to be stored here.

    The split is also what keeps the controls usable. The plot's sensor senses
    clicks, and a sensor takes every click over it; while the plot was drawn
    inline the sensor spanned the whole panel and swallowed the presses meant
    for the dropdowns. Now it can only ever cover the plot's own box.

    The explorer owns the three dropdowns and the plot. It does not tell the
    plot what to show — the plot reads the dropdowns itself — so all this node
    holds is the layout and the ids.
    """

    def __init__(self, columns, mode_dd, x_dd, y_dd, plot):
        self.columns = list(columns)
        self.mode_dd = mode_dd
        self.x_dd = x_dd
        self.y_dd = y_dd
        self.plot = plot

    def owned_nodes(self):
        return [self.mode_dd, self.x_dd, self.y_dd, self.plot]

    def on_delete(self, ctx):
        for uid in self.owned_nodes():
            ctx.workspace.delete_node(uid)

    def type_name(self):
        return "A Data Explorer"

    def request(self, req, ctx):
        """The query protocol, passed through to the plot.

        Another view wires to the explorer, because the explorer is the node it
        can see; the plot behind it is what knows where the marks went. So the
        split stays invisible from outside.
        """
        if getattr(req, "name", None) in PROTOCOL:
            return ctx.workspace.send_request(self.plot, req)
        return NotImplemented

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        h = base.y.provided_value() if base.y is not None else None
        if w is None or h is None or not (math.isfinite(w) and math.isfinite(h)):
            return dex.DrawResult.Complete(region=None)
        (ox, oy) = (base.pos.x, base.pos.y)

        # A page under everything, so the whole reads as one panel.
        _rect(ctx, ox, oy, w, h, PANEL, PANEL_EDGE)
        if not self.columns:
            _text(ctx, "no columns to explore", ox + MARGIN, oy + MARGIN,
                  TITLE_FONT, FAINT)
            return self._done(base, w, h)

        self._controls(ctx, ctx.node.workspace, ox, oy)

        # The plot, in what the control bar left. Both axes bounded, which is
        # what makes the box its geometry and its probe region alike.
        px = ox + MARGIN
        py = oy + CONTROL_H + CONTROL_GAP
        pw = w - 2.0 * MARGIN
        ph = h - (CONTROL_H + CONTROL_GAP) - MARGIN
        if pw > 40.0 and ph > 40.0:
            ctx.draw_inspectable_node(self.plot, _at(px, py, pw, ph))
        return self._done(base, w, h)

    def _done(self, base, w, h):
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))

    def _controls(self, ctx, ws, ox, oy):
        """The dropdowns across the top.

        The y column only has a say in bivariate mode, so its dropdown only
        stands there — the one piece of the view this node has to read back,
        because it decides the layout rather than the picture.
        """
        x = ox + MARGIN
        y = oy + (CONTROL_H - DD_H) / 2.0
        ctx.draw_node(self.mode_dd, _at(x, y, DD_MODE_W, DD_H))
        x += DD_MODE_W + CONTROL_GAP

        _text(ctx, "x", x, oy + CONTROL_H / 2.0 - 6.0, TICK_FONT, FAINT)
        x += 12.0
        ctx.draw_node(self.x_dd, _at(x, y, DD_COL_W, DD_H))
        x += DD_COL_W + CONTROL_GAP

        mode_idx = ws.send_request(self.mode_dd, dex.DropdownSelection())
        if mode_idx is not None and MODES[mode_idx] == "Bivariate":
            _text(ctx, "y", x, oy + CONTROL_H / 2.0 - 6.0, TICK_FONT, FAINT)
            x += 12.0
            ctx.draw_node(self.y_dd, _at(x, y, DD_COL_W, DD_H))


# ======================================================================
# Building
# ======================================================================


def build(ws, source=None, mode=None, x=None, y=None):
    """A `DataExplorer` node over `source`; returns the node object.

    `mode`, `x` and `y` open it on a particular view — a mode name and two column
    names. Anything unrecognised falls back to a sensible default: univariate,
    over the first column, with a different second column so bivariate does not
    start as x against itself.
    """
    columns, data, kinds = read_table(source)

    # The opening view: a mode, an x column, and a y column that differs from x.
    mode_i = MODES.index(mode) if mode in MODES else 0
    x_i = columns.index(x) if x in columns else 0
    y_i = columns.index(y) if y in columns else (1 if len(columns) > 1 else 0)

    mode_dd = dex.Dropdown.build(ws, list(MODES))
    x_dd = dex.Dropdown.build(ws, list(columns) or ["—"])
    y_dd = dex.Dropdown.build(ws, list(columns) or ["—"])
    if mode_i:
        ws.submit_action(mode_dd, dex.SetDropdownSelection(mode_i))
    if x_i:
        ws.submit_action(x_dd, dex.SetDropdownSelection(x_i))
    if y_i:
        ws.submit_action(y_dd, dex.SetDropdownSelection(y_i))
    # One sensor over the plot, and only over the plot: hover to read a mark,
    # click to select it. It does not sense drags, so on a plane the surface
    # still pans.
    sensor = ws.insert_node_dyn(dex.InteractionBox.sensing(True, True, False))

    plot = Plot(columns, data, kinds, mode_dd, x_dd, y_dd, sensor)
    # Mirror the opening view, so the first frame agrees with the dropdowns.
    plot.mode = MODES[mode_i]
    if columns:
        plot.x_col = columns[x_i]
        plot.y_col = columns[y_i]
    return DataExplorer(columns, mode_dd, x_dd, y_dd, ws.insert_node_dyn(plot))


def transform():
    """A data explorer over the wired `thisData`, or over the built-in sample."""
    return build(
        dex.ws,
        globals().get("thisData"),
        globals().get("explorer_mode"),
        globals().get("explorer_x"),
        globals().get("explorer_y"),
    )
