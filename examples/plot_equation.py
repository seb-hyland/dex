"""Plot an equation by running it"""

import math
import random

SAMPLES = 480
#: How many points a surface is sampled at along each side. The grid is the
#: square of this, so it is much smaller.
SURFACE_STEPS = 26
#: The range sampled when nothing says otherwise.
DEFAULT_SPAN = (-10.0, 10.0)
#: How far the picture reaches past that span, as a multiple of it. Finite
#: because a picture has a size; generous because reaching the end of one by
#: scrolling should take a while.
REACH = 4.0
#: How much is evaluated at a time once the window passes the sampled edge, as a
#: fraction of the span. A page rather than exactly what is showing, so a slow
#: drag does not re-enter the sampler every frame for a pixel of new curve.
PAGE = 0.25

#: What the plane asks the curve to cover. A request rather than an action, for
#: the reasons `SetSelection` gives: scrolling is not an edit, and an action is
#: applied to a *deep copy* of the node it reaches — which for a view holding the
#: whole sampled table would mean copying it on every frame of a drag.
SAMPLE_TO = "plot_equation.sample_to"


class SampleTo(dex.Request):
    """Cover these value windows, computing whatever is not covered yet.

    `{channel: (lo, hi)}`, in data units, for every continuous axis the view
    published. The sender does not know or care which of them the view was
    sampled along; the view takes the one it recognises and ignores the rest.
    """

    name = SAMPLE_TO

    def __init__(self, windows):
        self.windows = windows


def plotted_variable(runner):
    """`"x"`, `"y"`, or `"xy"` — which of them this lambda is a function of."""
    names = [name for name in runner.params if name in ("x", "y")]
    if not names:
        raise Unrunnable(
            "this plots a function of x or y, and that lambda takes %s"
            % (", ".join(runner.params) or "nothing"))
    if len(names) == 2:
        return "xy"
    return names[0]


def sample_curve(runner, variable, lo, hi, count=SAMPLES):
    """`([...], [...])` over `[lo, hi]`, skipping what will not evaluate.

    A function is allowed to be undefined somewhere — a division by zero, a root
    of a negative — and one bad sample should cost that sample rather than the
    plot. So each is guarded, and the gap simply is not drawn.
    """
    step = (hi - lo) / max(count - 1, 1)
    (ins, outs) = ([], [])
    for i in range(count):
        value = lo + step * i
        try:
            got = runner(**{variable: value})
        except Exception:
            continue
        if not is_num(got):
            continue
        ins.append(value)
        outs.append(float(got))
    return (ins, outs)


def curve_columns(variable, ins, outs):
    """What was sampled and what came back, as columns, the right way round.

    A lambda of `y` is a curve lying on its side: what it returns is the *x* of
    each point. Naming the columns for the axes they belong on rather than for
    the roles they played keeps everything downstream — the scatter, the
    readout, a join — talking about x and y.

    Separate from `curve_table` because every later page of curve has to be
    turned round the same way, and there is only one place that should know
    which way round that is.
    """
    if variable == "y":
        return {"x": outs, "y": ins}
    return {"x": ins, "y": outs}


def curve_table(runner, variable, lo, hi):
    """The curve over `[lo, hi]` as columns."""
    return curve_columns(variable, *sample_curve(runner, variable, lo, hi))


def surface_table(runner, lo, hi, count=SURFACE_STEPS):
    """A function of both, sampled on a grid: `{x, y, z}`."""
    step = (hi - lo) / max(count - 1, 1)
    rows = {"x": [], "y": [], "z": []}
    for i in range(count):
        for j in range(count):
            (x, y) = (lo + step * i, lo + step * j)
            try:
                got = runner(x=x, y=y)
            except Exception:
                continue
            if not is_num(got):
                continue
            rows["x"].append(x)
            rows["y"].append(y)
            rows["z"].append(float(got))
    return rows


class EquationPlot(Scatter):
    """The sampled function, drawn as the library's scatter with a line through.

    Everything a scatter does is wanted here — the axes, the picking, the hover
    readout, the whole protocol — so this is that view, with two things turned
    round. The points are joined in the order they were sampled, because a
    function *is* a line and the samples are only where it was looked at. And
    there is no least-squares fit, because there is nothing to fit: a regression
    line through a curve worked out from a formula is a statement about noise
    that is not there.
    """

    KIND = "equation"
    FIT = False
    # Configuration of its own, declared at class level as well as set in
    # `__init__`, so an instance that `become`s this finds sane values rather
    # than missing ones. See `Plot.become`.
    variable = "x"
    #: The span of the sampled axis the picture covers, pinned at build time and
    #: never touched again. What `x_bounds`/`y_bounds` answer with.
    domain = (0.0, 1.0, 0.5)
    #: How much of that domain has actually been evaluated, as `(lo, hi)`.
    sampled = (0.0, 1.0)
    #: Samples per unit, and how much is taken at a time.
    density = 1.0
    page = 1.0
    #: The prepared lambda. Not saved: see `__getstate__`.
    runner = None

    def __init__(self, frame, sensor, variable="x", **encoding):
        self.variable = variable if variable in ("x", "y") else "x"
        # Joined along whichever variable it was sampled across: a curve on its
        # side is in order of its y, and sorting it by x would zig-zag.
        super().__init__(frame, sensor, connect=self.variable, **encoding)

    def type_name(self):
        return "y = f(x)" if self.variable == "x" else "x = f(y)"

    def __getstate__(self):
        """Everything but the runner, which holds compiled code and will not
        pickle. A reopened workspace keeps the curve and stops extending it."""
        state = super().__getstate__()
        state["runner"] = None
        return state

    # -- the pinned mapping ----------------------------------------------

    def x_bounds(self, values):
        """The domain, if x is what this was sampled along; else the data's.

        The whole of why scrolling does not run away: on the sampled axis this
        answers the same three numbers whatever has been computed so far, so the
        mapping the window is read through is a constant.
        """
        if self.variable == "x":
            return self.domain
        return super().x_bounds(values)

    def y_bounds(self, values):
        """The same the other way round, for a curve lying on its side."""
        if self.variable == "y":
            return self.domain
        return super().y_bounds(values)

    # -- filling it in ---------------------------------------------------

    def request(self, req, ctx):
        if getattr(req, "name", None) == SAMPLE_TO:
            return self.cover(req.windows.get(self.variable))
        return super().request(req, ctx)

    def cover(self, window):
        """Evaluate out to `window`, in whole pages, and never back in again.

        Monotone on purpose. Shrinking to what is on screen would mean the curve
        vanished from ground already paid for the moment you scrolled off it,
        and — worse — it would make what is sampled a function of the view
        again, which is the loop this whole file is arranged to avoid.
        """
        if window is None or self.runner is None:
            return False
        (lo, hi) = self.sampled
        (dlo, dhi, _step) = self.domain
        want_lo = max(dlo, min(window[0], hi))
        want_hi = min(dhi, max(window[1], lo))
        # A hair of slack, so a window sitting flush against the edge it was
        # placed to sit flush against does not buy a page on the first frame.
        slack = self.page * 1e-3
        # Out in whole pages, so a drag crosses the edge once rather than once
        # per frame.
        new_lo = lo if want_lo >= lo - slack else max(dlo, lo - self.page * math.ceil(
            (lo - want_lo) / self.page))
        new_hi = hi if want_hi <= hi + slack else min(dhi, hi + self.page * math.ceil(
            (want_hi - hi) / self.page))
        if new_lo >= lo and new_hi <= hi:
            return False

        added = []
        if new_lo < lo:
            added.append(sample_curve(self.runner, self.variable, new_lo, lo,
                                      self.count_over(lo - new_lo)))
        if new_hi > hi:
            added.append(sample_curve(self.runner, self.variable, hi, new_hi,
                                      self.count_over(new_hi - hi)))
        self.sampled = (new_lo, new_hi)
        self.absorb(added)
        return True

    def count_over(self, width):
        """How many samples a stretch that wide gets, at the density the first
        screenful was drawn at — so new curve is no coarser than old."""
        return max(2, int(round(width * self.density)))

    def absorb(self, batches):
        """Add the new points to the table, keeping every row id it already had.

        Appended, never merged in order: a row id is what a selection, a hover
        and a linked view all name a point by, and inserting on the left would
        quietly renumber every one of them. `Scatter` sorts by the connected
        channel when it draws the line, so the stored order does not matter.
        """
        xs = list(self.frame.values("x"))
        ys = list(self.frame.values("y"))
        for (ins, outs) in batches:
            batch = curve_columns(self.variable, ins, outs)
            xs.extend(batch["x"])
            ys.extend(batch["y"])
        if len(xs) == self.frame.n:
            return
        self.frame = Frame({"x": xs, "y": ys})
        # `Plot` keeps one of these per row, and Strip reads it by index — so a
        # view that grew and then `become`s one would run off the end of it.
        self._jit = [random.Random(i * 2654435761).uniform(-1.0, 1.0)
                     for i in range(self.frame.n)]


class Window(PlaneChrome):
    """Watches what the plane is showing and asks the curve to cover it.

    Chrome rather than something the plot does itself, because knowing what is
    on screen means knowing the plane's pan, its zoom *and* the size of the
    viewport — and the viewport is the one thing a view drawn inside a canvas
    item cannot see. A background is handed it as its own box, which is exactly
    why the axes and the readout live out here too.

    It paints nothing at all. What it does is read one rectangle and pass it on.
    """

    def type_name(self):
        return "The Sampled Window"

    def draw(self, ctx):
        view = self.view(ctx)
        scale = self.scale(ctx)
        if view is None or not scale:
            return self.nothing()
        (origin, zoom, base, w, h) = view

        windows = {}
        for (channel, near, extent) in (("x", origin.x, w), ("y", origin.y, h)):
            axis = scale.get(channel)
            if not axis or axis.get("kind") == "category":
                continue
            # Through the view's own mapping, the same way the gridlines are
            # placed — and on the sampled axis that mapping is a constant.
            (a, b) = (axis_value(axis, near), axis_value(axis, near + extent / zoom))
            windows[channel] = (min(a, b), max(a, b))
        if windows:
            ctx.node.workspace.send_request(self.plot, SampleTo(windows))
        return self.nothing()


def picture_box(plot, span):
    """How big the picture is, and where its corner goes on the plane.

    Sized so that a unit of the sampled axis is the same number of pixels it
    would have been in a `PLANE_SIZE` picture of `span` alone — the scale the
    plot opens at is the scale it would always have had — and then slid so that
    `span` is exactly what the plane's first screenful shows.
    """
    (dlo, dhi, _step) = plot.domain
    (slo, shi, _sstep) = nice_bounds(span[0], span[1])
    (vw, vh) = PLANE_SIZE
    if plot.variable == "x":
        # x runs left to right, from the gutter to the picture's right edge.
        px = (vw - GUTTER) / ((shi - slo) or 1.0)
        return ((GUTTER + (dhi - dlo) * px, vh),
                dex.Vector.new(-(slo - dlo) * px, 0.0))
    # y runs bottom to top, between the foot and the head — and upside down, so
    # it is the *far* end of the domain that has to line up with the top edge.
    px = (vh - TOP - FOOT) / ((shi - slo) or 1.0)
    return ((vw, TOP + FOOT + (dhi - dlo) * px),
            dex.Vector.new(0.0, -(dhi - shi) * px))


def equation_plane(ws, plot, span, name=None):
    """`plot_on_plane`, for a picture bigger than one screenful.

    The library's version puts the view at the plane's origin at `PLANE_SIZE`,
    which *is* one screenful — there is nowhere to scroll to. This makes the
    picture the whole reachable domain instead and slides it so the span you
    asked for is what you open on. Everything else is `plot_on_plane`'s: the
    same four chrome nodes, and one more that does the sampling.
    """
    plot.chrome = False
    body = ws.insert_node_dyn(plot)
    (size, corner) = picture_box(plot, span)
    canvas = dex.Canvas.build(ws)
    item = dex.StaticCanvasItem.build(
        ws, body, corner, dex.Vector.new(size[0], size[1]))
    ws.submit_action(canvas, dex.AdoptCanvasNode(item, dex.Layer.midground()),
                     "Placed the curve")
    ws.submit_action(canvas, dex.NameCanvas(name=name or plot.type_name()),
                     "Named the plane")
    adopt(ws, canvas, PlotAxes(canvas, body), dex.Layer.background())
    adopt(ws, canvas, PlotTitle(canvas, body), dex.Layer.foreground())
    adopt(ws, canvas, PlotStatsPanel(canvas, body), dex.Layer.foreground())
    adopt(ws, canvas, PlotChrome(body), dex.Layer.foreground())
    # Last, and it draws nothing: it only reads what the plane is showing and
    # asks the curve to cover it. After the view, so what it reads is this
    # frame's mapping rather than the one before it.
    adopt(ws, canvas, Window(canvas, body), dex.Layer.foreground())
    return canvas


def build_equation_plot(ws, equation, span=DEFAULT_SPAN):
    """Sample `equation` and put the curve — or the surface — on a plane."""
    runner = LambdaRunner(dex.snapshot, equation)
    variable = plotted_variable(runner)

    if variable == "xy":
        # A function of both is a surface, so it gets the projection rather than
        # a curve: three columns, turned by dragging. Not scrollable: a surface
        # is sampled on a grid, and reaching further out in both directions at
        # once costs the square of what a curve does.
        plot = build_plot(ws, Scatter3D, surface_table(runner, span[0], span[1]),
                          x="x", y="y", z="z")
        return plot_on_plane(ws, plot, name="z = f(x, y)")

    plot = build_plot(ws, EquationPlot,
                      curve_table(runner, variable, span[0], span[1]),
                      x="x", y="y", variable=variable)
    # Everything the sampler needs, settled once: the reachable domain, what has
    # been evaluated inside it so far, and at what resolution.
    width = (span[1] - span[0]) or 1.0
    pad = width * (REACH - 1.0) / 2.0
    plot.domain = nice_bounds(span[0] - pad, span[1] + pad)
    plot.sampled = (span[0], span[1])
    plot.density = SAMPLES / width
    plot.page = width * PAGE
    plot.runner = runner
    return equation_plane(ws, plot, span)


def transform():
    """The curve of the lambda wired into `equation`."""
    if equation is None:
        raise ValueError("wire a lambda taking x or y into this transform")
    return build_equation_plot(dex.ws, equation)
