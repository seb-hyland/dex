"""A line plot."""

SIZE = (760.0, 480.0)
BODY = (56.0, 24.0, 680.0, 408.0)

#: What the two axes map.
X = (0.0, 10.0, 2.0)        # (lo, hi, step), in seconds
Y = (0.0, 25.0, 5.0)        # in metres
X_TITLE = "Seconds"
Y_TITLE = "Metres"

TICK_GAP = 6.0
LINE_W = 2.0


def place(axis, value, p0, p1):
    """Where `value` falls between the pixels `p0` and `p1`.

    `p0 > p1` is how y comes out the right way up: hand it the bottom, then
    the top.
    """
    (lo, hi, _step) = axis
    return p0 + (value - lo) / (hi - lo) * (p1 - p0)


class LinePlot:
    """The paper the line stands on: its grid, its ticks and its captions."""

    def __init__(self, x_title, y_title):
        self.x_title = x_title
        self.y_title = y_title

    def owned_nodes(self):
        return [self.x_title, self.y_title]

    def on_delete(self, ctx):
        for uid in self.owned_nodes():
            ctx.workspace.delete_node(uid)

    def type_name(self):
        return "A Line Plot"

    def draw(self, ctx):
        (x, y) = (ctx.constraints.pos.x, ctx.constraints.pos.y)
        (bx, by, bw, bh) = (x + BODY[0], y + BODY[1], BODY[2], BODY[3])
        rect(ctx, x, y, SIZE[0], SIZE[1], PANEL, PANEL_EDGE, 3.0)

        for value in axis_ticks(X[0], X[1], X[2]):
            sx = place(X, value, bx, bx + bw)
            line(ctx, [(sx, by), (sx, by + bh)], GRID, 1.0)
            caption = tick_text(value, X[2])
            (cw, _ch) = measure(ctx, caption, TICK_FONT)
            text(ctx, caption, sx - cw / 2.0, by + bh + TICK_GAP, TICK_FONT, FAINT)

        for value in axis_ticks(Y[0], Y[1], Y[2]):
            sy = place(Y, value, by + bh, by)
            line(ctx, [(bx, sy), (bx + bw, sy)], GRID, 1.0)
            caption = tick_text(value, Y[2])
            (cw, ch) = measure(ctx, caption, TICK_FONT)
            text(ctx, caption, bx - TICK_GAP - cw, sy - ch / 2.0, TICK_FONT, FAINT)

        line(ctx, [(bx, by), (bx, by + bh)], AXIS, 1.0)
        line(ctx, [(bx, by + bh), (bx + bw, by + bh)], AXIS, 1.0)

        # Unbounded, so each caption is only as wide as what is in it — and so
        # an editable one sits where it is put rather than centring in a band.
        (tw, _th) = measure(ctx, X_TITLE, TITLE_FONT, True)
        ctx.draw_inspectable_node(self.x_title, at(bx + (bw - tw) / 2.0, by + bh + 22.0))
        ctx.draw_inspectable_node(self.y_title, at(x + 8.0, y + 6.0))
        return dex.DrawResult.Complete(region=dex.ScreenRegion.from_min_size(
            ctx.constraints.pos, dex.Vector.new(*SIZE)))


def caption(ws, s):
    """An axis caption"""
    field = dex.Label.new(s)
    font = dex.Font.proportional(TITLE_FONT)
    font.bold = True
    field.font = font
    field.color = dex.Color.rgb(*INK)
    return ws.insert_node_dyn(field)


def steady_pace():
    """The samples behind the line: a steady 2.5 m/s, as `[(t, distance)]`."""
    return [(t, 2.5 * t) for t in (0.0, 2.0, 4.0, 6.0, 8.0, 10.0)]


def transform():
    """The plot, the line over it, and the plane the two of them sit on."""
    ws = dex.ws
    plot = ws.insert_node_dyn(LinePlot(caption(ws, X_TITLE), caption(ws, Y_TITLE)))
    canvas = on_plane(ws, plot, SIZE, "A Line Plot")

    # The line, in the body's own coordinates, held at the body's corner.
    points = [dex.Vector.new(place(X, t, 0.0, BODY[2]), place(Y, d, BODY[3], 0.0))
              for (t, d) in steady_pace()]
    path = ws.insert_node_dyn(dex.Path.polyline(
        points, dex.Stroke.new(LINE_W, dex.Color.rgb(*LINE))))
    editor = dex.PathEditor.build(
        ws, path, dex.Vector.new(BODY[0], BODY[1]), False, True)
    ws.submit_action(canvas, dex.AdoptCanvasNode(editor, dex.Layer.midground()),
                     "Added the line")

    # Open looking at the middle of the picture rather than at its corner.
    ws.submit_action(canvas, dex.CentreCanvasView(
        dex.Vector.new(SIZE[0] / 2.0, SIZE[1] / 2.0)), "Centred the view")
    return canvas
