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

**Sampled once, over a fixed range, and then left alone.** The curve is a table
like any other: `x` and `y` columns, worked out at build time, handed to the
library's `Scatter`, and put on a plane. Panning and zooming the plane moves the
picture, and that is all it does — the mapping from a value to a place on the
plane never changes, so the curve stays where you left it.

That is worth saying because the obvious alternative does not work. Re-sampling
whatever range is currently on screen sounds like a way to get more curve as you
zoom in; but the range on screen is read *through* the view's own axis, and the
axis is worked out from the data the last sample produced. Each frame then
re-derives the window from a mapping the previous frame's answer had already
moved, and since the axis rounds outward to a round number the two chase each
other outward — a curve that flies apart on its own while nobody touches it.
A fixed mapping has no loop in it to run away.
"""

#: How many points the curve is sampled at across the range. Generous, because
#: it is sampled once: this is the resolution of the curve at every zoom.
SAMPLES = 480
#: How many points a surface is sampled at along each side. The grid is the
#: square of this, so it is much smaller.
SURFACE_STEPS = 26
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
    # See `Plot.become`.
    variable = "x"

    def __init__(self, frame, sensor, variable="x", **encoding):
        self.variable = variable if variable in ("x", "y") else "x"
        # Joined along whichever variable it was sampled across: a curve on its
        # side is in order of its y, and sorting it by x would zig-zag.
        super().__init__(frame, sensor, connect=self.variable, **encoding)

    def type_name(self):
        return "y = f(x)" if self.variable == "x" else "x = f(y)"


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

    plot = build_plot(ws, EquationPlot,
                      curve_table(runner, variable, span[0], span[1]),
                      x="x", y="y", variable=variable)
    return plot_on_plane(ws, plot)


def transform():
    """The curve of the lambda wired into `equation`."""
    equation = globals().get("equation")
    if equation is None:
        raise ValueError("wire a lambda taking x or y into this transform")
    return build_equation_plot(dex.ws, equation)
