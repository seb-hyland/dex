"""Plot an equation by running it.

Wire a lambda into `equation` — one built by `gen_eq.py`, or any lambda at all —
and this samples it and draws the curve. Nothing about the equation is read
symbolically: it is *run*, once per sample, and what comes back is plotted.

**Which variable is which comes from the lambda's own parameters.** A lambda
taking `x` is a curve `y = f(x)`, sampled across x. One taking `y` is the same
curve lying on its side, `x = f(y)`, sampled across y. One taking *both* is not
a curve at all — it is a surface `z = f(x, y)` — so it is sampled on a grid and
drawn as a 3D cloud instead, which is what a surface actually looks like. A
lambda taking neither cannot be plotted, and says so, naming what it does take.

**Running it many times is the whole problem.** Wiring a value in and reading
the output back computes once, on a worker, and answers a frame later; and
running a lambda the ordinary way runs the whole workspace prelude again each
time. So this uses the prelude's `LambdaRunner`, which reads the lambda's graph
once, compiles every script in it once, and then evaluates cheaply — memoising
as it goes, so re-sampling ground already covered costs nothing.

That memo is what makes the plot follow the plane. The curve is sampled over the
range currently on screen, so zooming in gets you *more* of the curve rather
than the same points further apart — and panning back over somewhere you have
already been redraws from the cache.
"""

#: How many points the curve is sampled at across the visible range.
SAMPLES = 240
#: How many points a surface is sampled at along each side. The grid is the
#: square of this, so it is much smaller.
GRID = 26
#: The range sampled when nothing says otherwise.
DEFAULT_SPAN = (-10.0, 10.0)


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
    """`{"in": [...], "out": [...]}` over `[lo, hi]`, skipping what will not
    evaluate.

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


def curve_table(runner, variable, lo, hi):
    """The sampled curve as columns, with x and y the right way round.

    A lambda of `y` is a curve lying on its side: what it returns is the *x* of
    each point. Naming the columns for the axes they belong on rather than for
    the roles they played keeps everything downstream — the scatter, the
    readout, a join — talking about x and y.
    """
    (ins, outs) = sample_curve(runner, variable, lo, hi)
    if variable == "y":
        return {"x": outs, "y": ins}
    return {"x": ins, "y": outs}


def surface_table(runner, lo, hi, count=GRID):
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


class EquationPlot(Plot):
    """A scatter of a sampled function, re-sampled for whatever is on screen.

    A `Scatter` whose table is not given but *made*, and made again whenever the
    window over it moves. The runner underneath memoises, so what changes
    between one frame and the next is only the samples that are new.
    """

    KIND = "equation"
    CHANNELS = ("x", "y")

    def __init__(self, frame, sensor, runner=None, variable="x",
                 span=DEFAULT_SPAN, **encoding):
        super().__init__(frame, sensor, **encoding)
        self.runner = runner
        self.variable = variable
        #: The range last sampled, so a frame that has not moved re-uses it.
        self.span = tuple(span)

    def type_name(self):
        return "y = f(x)" if self.variable == "x" else "x = f(y)"

    def resample(self, lo, hi):
        """Sample `[lo, hi]` and take the result as this view's table."""
        if self.runner is None:
            return
        (lo, hi) = (min(lo, hi), max(lo, hi))
        if hi - lo <= 0.0:
            return
        self.span = (lo, hi)
        self.frame = Frame(curve_table(self.runner, self.variable, lo, hi))
        self.n = self.frame.n
        self.enc["x"] = "x"
        self.enc["y"] = "y"

    def paint(self, ctx, x, y, w, h):
        # The same marks a scatter draws; the table under them is what differs.
        Scatter.paint(self, ctx, x, y, w, h)


class EquationSampler:
    """Keeps the curve in step with the part of the plane you are looking at.

    A background, because that is where the view's own mapping and the plane's
    view origin are both readable — and because it has to run before the view
    draws, which the background band is.
    """

    def __init__(self, canvas, plot):
        self.canvas = canvas
        self.plot = plot

    def owned_nodes(self):
        return []

    def type_name(self):
        return "An Equation Sampler"

    def draw(self, ctx):
        ws = ctx.node.workspace
        origin = ws.send_request(self.canvas, dex.CanvasViewOrigin())
        zoom = ws.send_request(self.canvas, dex.CanvasZoom()) or 1.0
        scale = ws.send_request(self.plot, PlotScale())
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        if origin is None or w is None or not scale or zoom <= 0.0:
            return dex.DrawResult.Complete(region=None)

        across = scale.get("x")
        if not across or across.get("kind") != "value":
            return dex.DrawResult.Complete(region=None)
        # What the two edges of the window are worth in the data.
        lo = axis_value(across, origin.x)
        hi = axis_value(across, origin.x + w / zoom)
        ws.send_request(self.plot, Resample(lo, hi))
        return dex.DrawResult.Complete(region=None)


class Resample(dex.Request):
    """Ask a plotted equation to cover `[lo, hi]`. Answers the range it took."""

    name = "dex.example.resample"

    def __init__(self, lo, hi):
        self.lo = lo
        self.hi = hi


class SampledPlot(EquationPlot):
    """An `EquationPlot` that answers `Resample`."""

    def request(self, req, ctx):
        if getattr(req, "name", None) == Resample.name:
            if abs(req.lo - self.span[0]) > 1e-9 or abs(req.hi - self.span[1]) > 1e-9:
                self.resample(req.lo, req.hi)
            return self.span
        return super().request(req, ctx)


def build_equation_plot(ws, equation, span=DEFAULT_SPAN):
    """Sample `equation` and put the curve — or the surface — on a plane."""
    runner = LambdaRunner(dex.snapshot, equation)
    variable = plotted_variable(runner)

    if variable == "xy":
        # A function of both is a surface, so it gets the projection rather than
        # a curve: three columns, turned by dragging.
        plot = build_plot(ws, Scatter3D, surface_table(runner, span[0], span[1]),
                          x="x", y="y", z="z")
        return plot_on_plane(ws, plot, name="z = f(x, y)")

    frame = Frame(curve_table(runner, variable, span[0], span[1]))
    sensor = ws.insert_node_dyn(dex.InteractionBox.sensing(True, True, False))
    plot = SampledPlot(frame, sensor, runner=runner, variable=variable,
                       span=span, x="x", y="y")
    plot.chrome = False
    body = ws.insert_node_dyn(plot)
    canvas = on_plane(ws, body, PLANE_SIZE, plot.type_name())
    # The sampler first: it runs in the background band, before the view draws,
    # so the curve it asks for is the curve that frame shows.
    adopt(ws, canvas, EquationSampler(canvas, body), dex.Layer.background())
    adopt(ws, canvas, PlotAxes(canvas, body), dex.Layer.background())
    adopt(ws, canvas, PlotTitle(canvas, body), dex.Layer.foreground())
    adopt(ws, canvas, PlotChrome(body), dex.Layer.foreground())
    return canvas


def transform():
    """The curve of the lambda wired into `equation`."""
    equation = globals().get("equation")
    if equation is None:
        raise ValueError("wire a lambda taking x or y into this transform")
    return build_equation_plot(dex.ws, equation)
