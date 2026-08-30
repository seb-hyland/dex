"""A gallery of `Path`-based shapes, laid out in a grid — a node defined
entirely in Python, the same way `tiled_layout.py` is.

Each shape is stored as plain data (an outline of anchors, a fill and a stroke
as RGB triples) rather than a live `dex.Path`: the geometry is just data, and
building the node at draw time keeps this gallery picklable. `draw` turns each
descriptor into a `dex.Path` and paints it into its cell, so every shape is
rendered by the real `Path` node — polygons, open polylines and Bézier curves
alike.
"""

import math

# Cell geometry, in node-local points.
COLS = 3
CELL_W = 250.0
CELL_H = 210.0
SHAPE_INSET = 30.0  # left padding of a shape within its cell
CAPTION_DROP = 150.0  # caption offset below the top of the cell


# -- shape descriptors ---------------------------------------------------
#
# A descriptor is `(name, fill, stroke, width, closed, anchors)` where:
#   * `fill`/`stroke` are RGB triples, or `None` for none;
#   * `closed` says whether the outline is filled and its ends joined;
#   * `anchors` is a list of `(x, y, in_handle, out_handle)`, each handle an
#     `(dx, dy)` offset from the point or `None` for a straight corner.


def _corners(points):
    """Anchors with no handles: a straight-edged outline through `points`."""
    return [(x, y, None, None) for (x, y) in points]


def _triangle():
    return (
        "triangle (polygon)",
        (80, 140, 240),
        (30, 60, 120),
        2.0,
        True,
        _corners([(60, 0), (120, 104), (0, 104)]),
    )


def _pentagon():
    r = 58.0
    points = [
        (r + r * math.cos(-math.pi / 2 + i * 2 * math.pi / 5),
         r + r * math.sin(-math.pi / 2 + i * 2 * math.pi / 5))
        for i in range(5)
    ]
    return ("pentagon (polygon)", (120, 200, 140), (40, 90, 60), 2.0, True, _corners(points))


def _star():
    outer, inner = 66.0, 27.0
    points = []
    for i in range(10):
        radius = outer if i % 2 == 0 else inner
        angle = -math.pi / 2 + i * math.pi / 5
        points.append((outer + radius * math.cos(angle), outer + radius * math.sin(angle)))
    return ("star (concave fill)", (250, 200, 60), (150, 110, 10), 2.0, True, _corners(points))


def _arrow():
    points = [(0, 30), (80, 30), (80, 0), (140, 55), (80, 110), (80, 80), (0, 80)]
    return ("arrow (polygon)", (230, 90, 90), None, 0.0, True, _corners(points))


def _wave():
    points = [(i * 4.5, 55.0 + 35.0 * math.sin(i * 4.5 / 22.0)) for i in range(31)]
    return ("wave (open polyline)", None, (70, 120, 220), 3.0, False, _corners(points))


def _blob():
    cx = cy = 62.0
    r = 56.0
    n = 6
    tangent = 2 * math.pi * r / n * 0.38
    anchors = []
    for i in range(n):
        angle = i * 2 * math.pi / n
        px, py = cx + r * math.cos(angle), cy + r * math.sin(angle)
        # A smooth vertex: the in handle mirrors the out handle.
        out = (-math.sin(angle) * tangent, math.cos(angle) * tangent)
        anchors.append((px, py, (-out[0], -out[1]), out))
    return ("blob (smooth anchors)", (180, 120, 230), (90, 50, 130), 2.0, True, anchors)


def _heart():
    # Four cubic segments with independent in/out handles.
    anchors = [
        (60, 108, (50, -38), (-50, -38)),
        (20, 20, (-20, 20), (15, -15)),
        (60, 28, (-5, -18), (5, -18)),
        (100, 20, (-15, -15), (20, 20)),
    ]
    return ("heart (bézier handles)", (230, 70, 100), (150, 30, 60), 2.0, True, anchors)


def _rounded_rect():
    # A rounded rectangle described purely as a Path: straight edges joined by
    # quarter-circle Bézier corners. This is `Rect`, expressed via `Path`.
    w, h, r = 150.0, 96.0, 22.0
    k = r * 0.5523  # cubic approximation of a quarter circle
    anchors = [
        (r, 0, (-k, 0), None),
        (w - r, 0, None, (k, 0)),
        (w, r, (0, -k), None),
        (w, h - r, None, (0, k)),
        (w - r, h, (k, 0), None),
        (r, h, None, (-k, 0)),
        (0, h - r, (0, k), None),
        (0, r, None, (0, -k)),
    ]
    return ("rounded rect (path arcs)", (90, 170, 210), (40, 90, 120), 2.0, True, anchors)


def default_shapes():
    """The built-in gallery, one descriptor per cell."""
    return [
        _triangle(), _pentagon(), _star(), _arrow(),
        _wave(), _blob(), _heart(), _rounded_rect(),
    ]


def _vec(x, y):
    return dex.Vector.new(float(x), float(y))


def build_path(spec):
    """Turn a descriptor into a live `dex.Path` node."""
    _name, fill, stroke, width, closed, anchors_data = spec

    anchors = []
    for (x, y, in_handle, out_handle) in anchors_data:
        anchor = dex.Anchor.corner(_vec(x, y))
        if in_handle is not None:
            anchor.in_handle = _vec(in_handle[0], in_handle[1])
        if out_handle is not None:
            anchor.out_handle = _vec(out_handle[0], out_handle[1])
        anchors.append(anchor)

    fill_color = dex.Color.rgb(*fill) if fill else dex.Color.transparent()
    stroke_style = dex.Stroke.new(width, dex.Color.rgb(*stroke)) if stroke else dex.Stroke.none()

    # `polygon` gives us a Path to populate; the fields carry the real outline.
    path = dex.Path.polygon([_vec(0, 0)], fill_color, stroke_style)
    path.anchors = anchors
    path.closed = closed
    return path


class ShapeGallery:
    def __init__(self, shapes=None):
        self.shapes = list(default_shapes() if shapes is None else shapes)

    # -- drawing ---------------------------------------------------------

    def draw(self, ctx):
        """Paint each shape into its grid cell and report the area taken."""
        origin = ctx.constraints.pos

        def box_at(x, y, w, h):
            return dex.DrawConstraints(
                pos=dex.ScreenPos.new(origin.x + x, origin.y + y),
                x=dex.AxisConstraint.Exactly(w),
                y=dex.AxisConstraint.Exactly(h),
                wrap=None,  # NotAllowed
                should_clip=False,
            )

        for i, spec in enumerate(self.shapes):
            col, row = i % COLS, i // COLS
            cell_x = col * CELL_W + SHAPE_INSET
            cell_y = row * CELL_H + 24.0
            ctx.draw_node(build_path(spec), box_at(cell_x, cell_y, CELL_W, CELL_H))
            ctx.draw_node(
                dex.Label.new(spec[0]),
                box_at(cell_x, cell_y + CAPTION_DROP, CELL_W, 20.0),
            )

        cols = min(len(self.shapes), COLS) or 1
        rows = (len(self.shapes) + COLS - 1) // COLS
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(
                origin,
                dex.Vector.new(cols * CELL_W + SHAPE_INSET, rows * CELL_H + 24.0),
            )
        )

    # -- lifecycle -------------------------------------------------------

    def type_name(self):
        return "Shape Gallery"


def transform():
    """A lambda returning the shape gallery."""
    return ShapeGallery()
