"""A blank white t-shirt, drawn to fill whatever box it is given.

This is an *image-style* node, not a canvas backdrop: it authors the shirt in a
fixed coordinate space (`ART_W` × `ART_H`) and maps that into the box it is
handed — uniform scale, centred, letterboxed — so it keeps its shape at any size,
the way an `<img>` does, rather than stretching or ruling a whole plane. Drop it
on a canvas and layer nodes over it to print the shirt.

Flat on purpose: a white body, a soft grey outline, a collar. The geometry is
plain data (anchor tuples), built into a real `dex.Path` at draw time, which
keeps the node picklable — the rule the other Python examples follow. Only the
left half is authored; the right is its mirror, so the tee is exactly symmetric.
"""

import math

# The coordinate space the shirt is authored in. Wide, the way a tee sits when
# it is laid out flat: sleeve-to-sleeve is a little more than shoulder-to-hem.
ART_W = 560.0
ART_H = 470.0
CENTER_X = 280.0

BODY = (255, 255, 255)
OUTLINE = (170, 174, 182)
COLLAR = (150, 154, 164)
OUTLINE_W = 3.4

# The left half of the outline, neck down to hem, as `(x, y, in_handle,
# out_handle)` — a handle is a `(dx, dy)` off the point, or `None` for a
# straight corner. The right half is this mirrored about `CENTER_X`, so the tee
# is exactly symmetric and only one side is ever edited.
#
# `in_handle` points back the way the outline came and `out_handle` on the way
# it is going, which is what the two control points of a cubic are. A tee is
# mostly straight lines with softened corners, so most of these are small.
SHIRT_HALF = [
    (228, 84, (6, 28), (-22, -2)),       # left of the neck
    (170, 68, (24, 6), (-28, 2)),        # shoulder point
    (76, 116, (30, -14), (-6, 22)),      # sleeve, where the shoulder seam ends
    (62, 198, (0, -22), (14, 16)),       # cuff, outer corner
    (134, 226, (-22, 6), (14, -14)),     # cuff, inner corner
    (180, 186, (-12, 18), None),         # underarm
    (172, 422, None, (18, 8)),           # side seam, at the hem
]
# The neckline dips between the two neck corners and closes the outline. The
# hem sags between the two side seams, which is `SHIRT_HALF`'s last out-handle
# meeting its own mirror.
NECK_DIP = (280, 138, (46, 0), (-46, 0))

# The collar band: the same dip again, a little lower and a little narrower.
COLLAR_LINE = [
    (239, 94, None, (5, 30)),
    (280, 157, (-46, 0), (46, 0)),
    (321, 94, (-5, 30), None),
]


def _vec(x, y):
    return dex.Vector.new(float(x), float(y))


def _mirror_anchor(a):
    """An anchor reflected across `CENTER_X`, for the far side of a symmetric
    outline traversed the other way: the two handles swap and flip in x."""
    (x, y, ih, oh) = a
    m_in = (-oh[0], oh[1]) if oh is not None else None
    m_out = (-ih[0], ih[1]) if ih is not None else None
    return (2.0 * CENTER_X - x, y, m_in, m_out)


def _mirrored_outline(half):
    """A closed outline from its left half: down one side, across, up the mirror."""
    right = [_mirror_anchor(a) for a in reversed(half)]
    return list(half) + right + [NECK_DIP]


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
        self._draw(ctx, box, self._path(_mirrored_outline(SHIRT_HALF), tf,
                                        fill=BODY, stroke=OUTLINE,
                                        width=OUTLINE_W, closed=True))
        self._draw(ctx, box, self._path(COLLAR_LINE, tf, stroke=COLLAR,
                                        width=OUTLINE_W * 0.7, closed=False))

    def type_name(self):
        return "A T-Shirt"


def transform():
    """A blank white t-shirt to draw on."""
    return TShirt()
