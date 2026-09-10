"""A blank t-shirt, traced from `demoenv/assets/tshirt.svg`.

Artwork designed by Vexels.com, 2017 — https://vexels.com/terms-and-conditions/

This is an *image-style* node, not a canvas backdrop: it authors the shirt in
the drawing's own coordinate space (`ART_W` × `ART_H`, which is the SVG's
viewBox) and maps that into the box it is handed — uniform scale, centred,
letterboxed — so it keeps its shape at any size, the way an `<img>` does, rather
than stretching or ruling a whole plane. Drop it on a canvas and layer nodes
over it to print the shirt.

Two things are worth knowing, and they are the same two `dynabook.py` makes.

**The ink is the SVG's own path data, verbatim, parsed at load.** Not a
hand-copy of it: what is below is the `d` attribute out of the file, so the
drawing stays auditable and the *curves* survive. A dex path carries Bezier
handles and flattens them at the size it is actually drawn, so the outline stays
smooth however far this is zoomed in; pre-flattening to points here would bake
in one resolution.

**The shirt is an outline, which means a hole.** The path is two loops — the
silhouette, and the same shape again just inside it — and SVG fills the band
between them by winding. A dex path has no notion of winding: it fills the one
loop it is given. So the two are painted in order instead, the outer in ink and
the inner in paper over the top, which leaves exactly the band. With two nested
loops, area says which is which and nothing has to be inferred.

The inside is painted rather than left clear so that the shirt is white paper to
print on at any magnification, instead of whatever happens to be behind it.
"""

import math
import re

# The SVG's viewBox, which is the space the path data below is written in.
ART_W = 1200.0
ART_H = 1200.0

#: The artwork's own fill, `#2A2C2C`, and the paper inside it.
INK = (42, 44, 44)
BODY = (255, 255, 255)

# The `d` attribute of the one path in `demoenv/assets/tshirt.svg`, copied
# across unchanged. Its first loop is the outside of the shirt and its second is
# the inside; between them is everything that gets drawn.
SHIRT = """\
M919.349,1150h-638.69V507.479l-14.457,66.461L70.304,500.784v-7.119c0-2.253,0-8.246,110.073-312.584
l1.352-3.725L422.773,50.015l4.271,10.508c1.938,4.744,48.745,116.356,173.441,122.391C724.6,176.86,772.471,61.7,772.941,60.538
L777.192,50l241.079,127.356l1.352,3.725c110.073,304.338,110.073,310.331,110.073,312.584v7.119L933.799,573.94l-14.45-66.434
V1150z M301.162,1129.493H898.85V316.685l49.941,229.759l158.118-59.034c-12.286-37.342-67.002-189.9-105.217-295.629
L787.075,78.407c-15.884,30.544-71.783,119.43-186.59,124.986c-115.581-5.557-171.647-94.442-187.561-124.986L198.318,191.78
c-38.229,105.729-92.941,258.287-105.231,295.629l158.111,59.034l49.964-229.729V1129.493z"""

# Every command the file actually uses. A tee traced from a photograph is mostly
# straight runs with a few curves, so the axis-aligned shorthands (`H`/`V`) turn
# up as often as the general lineto does.
_TOKENS = re.compile(r"([MmLlHhVvCcZz])|(-?[0-9]*\.?[0-9]+(?:[eE][-+]?[0-9]+)?)")


def _parse(d):
    """SVG path data into loops of `(x, y, in_handle, out_handle)` anchors.

    A handle is the offset from its own anchor to a Bezier control point, which
    is exactly what `dex.Anchor` wants — so a cubic's first control belongs to
    the anchor it leaves and its second to the anchor it arrives at.

    A loop that closes by landing back on the point it started from leaves a
    duplicate anchor sitting on the first one. That last anchor's in-handle is
    the closing segment's control, so it is moved onto the first anchor and the
    duplicate dropped.
    """
    tokens = _TOKENS.findall(d)
    loops, anchors = [], []
    (cx, cy) = (0.0, 0.0)
    (sx, sy) = (0.0, 0.0)
    cmd = None
    numbers = []

    def flush():
        """Close off the loop being built."""
        if len(anchors) < 2:
            return
        (x, y, hin, _hout) = anchors[-1]
        if abs(x - anchors[0][0]) < 1e-6 and abs(y - anchors[0][1]) < 1e-6:
            (fx, fy, _fin, fout) = anchors[0]
            anchors[0] = (fx, fy, hin, fout)
            anchors.pop()
        loops.append(list(anchors))

    for (op, num) in tokens:
        if not num:
            numbers = []
            cmd = op
            if op in "Zz":
                flush()
                anchors.clear()
                (cx, cy) = (sx, sy)
            continue
        numbers.append(float(num))

        # A command takes its arguments a fixed number at a time and repeats for
        # as long as the numbers keep coming: two cubics in a row are written
        # once, with twelve numbers after the one letter.
        if cmd in "Mm" and len(numbers) == 2:
            flush()
            anchors.clear()
            (cx, cy) = ((numbers[0], numbers[1]) if cmd == "M"
                        else (cx + numbers[0], cy + numbers[1]))
            (sx, sy) = (cx, cy)
            anchors.append((cx, cy, None, None))
            # Further pairs after a moveto are an implicit lineto.
            cmd = "L" if cmd == "M" else "l"
            numbers = []
        elif cmd in "Ll" and len(numbers) == 2:
            (cx, cy) = ((numbers[0], numbers[1]) if cmd == "L"
                        else (cx + numbers[0], cy + numbers[1]))
            anchors.append((cx, cy, None, None))
            numbers = []
        elif cmd in "HhVv":
            if cmd == "H":
                cx = numbers[0]
            elif cmd == "h":
                cx = cx + numbers[0]
            elif cmd == "V":
                cy = numbers[0]
            else:
                cy = cy + numbers[0]
            anchors.append((cx, cy, None, None))
            numbers = []
        elif cmd in "Cc" and len(numbers) == 6:
            (x1, y1, x2, y2, x, y) = numbers
            if cmd == "c":
                (x1, y1) = (cx + x1, cy + y1)
                (x2, y2) = (cx + x2, cy + y2)
                (x, y) = (cx + x, cy + y)
            if anchors:
                (px, py, pin, _pout) = anchors[-1]
                anchors[-1] = (px, py, pin, (x1 - px, y1 - py))
            anchors.append((x, y, (x2 - x, y2 - y), None))
            (cx, cy) = (x, y)
            numbers = []

    flush()
    return loops


def _area(anchors):
    """The shoelace area of a loop, ignoring its handles.

    Only ever compared against another loop's, and the two here differ by the
    whole of the shirt — so the corners are answer enough and the curves need
    not be flattened to ask.
    """
    total = 0.0
    for i, (x, y, _hin, _hout) in enumerate(anchors):
        (nx, ny, _nin, _nout) = anchors[(i + 1) % len(anchors)]
        total += x * ny - nx * y
    return abs(total) / 2.0


def _loops():
    """Every loop in the trace, with the colour it is painted, biggest first."""
    loops = sorted(_parse(SHIRT), key=_area, reverse=True)
    # Outermost first, so the inside is painted over the shape it is inside of.
    return [(loop, INK if i == 0 else BODY) for (i, loop) in enumerate(loops)]


def _vec(x, y):
    return dex.Vector.new(float(x), float(y))


class ImageNode:
    """A node that paints a fixed drawing scaled to fit its box.

    Subclasses set `art_w`/`art_h` and implement `paint(self, ctx, box, tf)`,
    where `tf(x, y)` maps an art point into the drawing's own frame and `tf.s`
    is the scale (for stroke widths). Everything about fitting-to-the-box lives
    here — including the clip, which is the drawing's frame rather than the box,
    so nothing paints out into the letterboxing.
    """

    art_w = ART_W
    art_h = ART_H

    def draw(self, ctx):
        origin = ctx.constraints.pos
        avail = ctx.constraints.available()
        (w, h) = (avail.x, avail.y)
        if not (math.isfinite(w) and math.isfinite(h)) or w <= 0.0 or h <= 0.0:
            (w, h) = (self.art_w, self.art_h)
        s = min(w / self.art_w, h / self.art_h)
        dx = (w - self.art_w * s) / 2.0
        dy = (h - self.art_h * s) / 2.0

        def tf(x, y):
            return _vec(x * s, y * s)
        tf.s = s

        # The box is the *drawing*, not the room it was given: an image does not
        # paint outside its own frame, and a node that clips to the whole box
        # lets a soft edge or a glow spill into the letterboxing beside it.
        box = dex.DrawConstraints(
            pos=dex.ScreenPos.new(origin.x + dx, origin.y + dy),
            x=dex.AxisConstraint.Exactly(self.art_w * s),
            y=dex.AxisConstraint.Exactly(self.art_h * s),
            wrap=None, should_clip=True)
        self.paint(ctx, box, tf)
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(origin, dex.Vector.new(w, h)))

    # -- painting helpers ------------------------------------------------

    def _path(self, anchors, tf, fill=None, stroke=None, width=0.0, closed=True):
        """A `dex.Path` from `(x, y, in, out)` anchors, mapped by `tf`."""
        built = []
        for (x, y, ih, oh) in anchors:
            a = dex.Anchor.corner(tf(x, y))
            if ih is not None:
                a.in_handle = _vec(ih[0] * tf.s, ih[1] * tf.s)
            if oh is not None:
                a.out_handle = _vec(oh[0] * tf.s, oh[1] * tf.s)
            built.append(a)
        fill_c = dex.Color.rgb(*fill) if fill else dex.Color.transparent()
        stroke_s = (dex.Stroke.new(width * tf.s, dex.Color.rgb(*stroke))
                    if stroke else dex.Stroke.none())
        path = dex.Path.polygon([_vec(0, 0)], fill_c, stroke_s)
        path.anchors = built
        path.closed = closed
        return path

    def _draw(self, ctx, box, path):
        ctx.draw_node(path, box)


class TShirt(ImageNode):
    def paint(self, ctx, box, tf):
        for (anchors, colour) in _loops():
            self._draw(ctx, box, self._path(anchors, tf, fill=colour))

    def type_name(self):
        return "A T-Shirt"


def transform():
    """A blank t-shirt to draw on."""
    return TShirt()
