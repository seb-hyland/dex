"""What a Wheatstone bridge *is*, drawn and nothing else.

Four arms in a diamond, a source across one diagonal and a detector across the
other. That is the whole of it, and this draws exactly that: no ratings, no
potentials, no currents, no reading on the meter. A schematic is a statement
about what is connected to what, and every number written on one is a statement
about some particular circuit instead.

So this takes no arguments and holds no state. It is a symbol, in the way `Ω` is
a symbol — the same picture whatever bridge you are thinking about, including
the one `wheatstone.py` builds and solves next door. The two files are not
wired together and there is nothing to wire: this one has no inputs, because a
diagram of a kind of circuit cannot have any.

It paints **no background**, so it sits on whatever it is put on and the surface
shows through. A drawing that brings its own paper can only ever be a rectangle
on a canvas; one that does not is a part of it.

**Everything is drawn from the node's own corner.** A shape carries absolute
points, so the origin the constraints hand over is added in as each one is
built. Nothing here needs to know where on a canvas it ended up.
"""

import math

#: The drawing's own size. Fixed, because a symbol has a shape rather than a
#: size that means something.
WIDTH = 476.0
HEIGHT = 384.0

# Where the diamond's corners and rails fall, in the node's own coordinates.
RAIL_TOP = 70.0
RAIL_BOTTOM = 274.0
COL_LEFT = 190.0
COL_RIGHT = 382.0
MIDDLE = 172.0
CENTRE = (COL_LEFT + COL_RIGHT) / 2.0
#: The source sits on a branch of its own, out to the left of the arms.
SOURCE_X = 78.0

#: How far a designator sits from the arm it names.
LABEL_GAP = 11.0
#: The detector's face.
METER_R = 22.0

STROKE = 1.6
WIRE = (74, 82, 96)
LEAD = (28, 32, 40)
LABEL_SIZE = 11.0


# =======================================================================
# Drawing, in the node's own frame
# =======================================================================
#
# Each of these takes the origin the constraints handed over and bakes it into
# the coordinates, because a path holds absolute points and is drawn from
# nowhere in particular. Written once here so the schematic below reads as
# geometry rather than as arithmetic about where the node happens to be.


def _rgb(c):
    return dex.Color.rgb(int(c[0]), int(c[1]), int(c[2]))


def _unplaced():
    """Constraints for a shape that carries its own coordinates."""
    return dex.DrawConstraints(
        pos=dex.ScreenPos.new(0.0, 0.0), x=None, y=None, wrap=None, should_clip=False
    )


def _at(x, y):
    return dex.DrawConstraints(
        pos=dex.ScreenPos.new(x, y), x=None, y=None, wrap=None, should_clip=False
    )


def _line(ctx, o, pts, colour=WIRE, width=STROKE):
    ctx.draw_node(
        dex.Path.polyline(
            [dex.Vector.new(o.x + x, o.y + y) for (x, y) in pts],
            dex.Stroke.new(width, _rgb(colour)),
        ),
        _unplaced(),
    )


def _dot(ctx, o, cx, cy, r, colour=WIRE):
    ctx.draw_node(dex.Circle.new(r, _rgb(colour)), _at(o.x + cx - r, o.y + cy - r))


def _ring(ctx, o, cx, cy, r, colour=WIRE, width=STROKE):
    ctx.draw_node(
        dex.Circle.bordered(
            r, dex.Color.transparent(), dex.Stroke.new(width, _rgb(colour))
        ),
        _at(o.x + cx - r, o.y + cy - r),
    )


def _font(size, bold):
    font = dex.Font.proportional(size)
    font.bold = bold
    return font


def _text(ctx, o, s, x, y, anchor="start", middle_y=False, size=LABEL_SIZE, bold=True):
    """A designator at `(x, y)`, anchored by its start, middle or end.

    `middle_y` centres it on the line as well, which is what a letter inside a
    symbol wants and what a label beside a wire does not.
    """
    measured = ctx.measure_text(s, _font(size, bold), dex.TextWrap.singleline())
    if anchor == "end":
        x -= measured.width
    elif anchor == "middle":
        x -= measured.width / 2.0
    if middle_y:
        y -= measured.height / 2.0
    label = dex.Label.new(s)
    label.font = _font(size, bold)
    label.color = _rgb(LEAD)
    ctx.draw_node(label, _at(o.x + x, o.y + y))


def _zigzag(x0, y0, x1, y1, teeth=6, amp=7.0):
    """A resistor's body: leads at each end and a zigzag between them."""
    (dx, dy) = (x1 - x0, y1 - y0)
    span = math.hypot(dx, dy) or 1.0
    (ux, uy) = (dx / span, dy / span)
    # The normal the teeth swing along.
    (nx, ny) = (-uy, ux)
    lead = span * 0.16
    body = span - 2.0 * lead
    step = body / (teeth * 2.0)
    pts = [(x0, y0), (x0 + ux * lead, y0 + uy * lead)]
    for i in range(1, teeth * 2):
        d = lead + i * step
        side = amp if i % 2 else -amp
        pts.append((x0 + ux * d + nx * side, y0 + uy * d + ny * side))
    pts.append((x1 - ux * lead, y1 - uy * lead))
    pts.append((x1, y1))
    return pts


# =======================================================================
# The circuit
# =======================================================================
#
# Read as a diamond stood on a pair of rails: the source across the top and
# bottom, the four arms down the two sides, and the detector bridging the
# midpoints — which is the arm the whole arrangement is named for.

#: `(designator, column, from, to, which side its name sits on)`. The names sit
#: on the outside of each column, away from the detector between them.
ARMS = [
    ("R1", COL_LEFT, RAIL_TOP, MIDDLE, "end"),
    ("R2", COL_LEFT, MIDDLE, RAIL_BOTTOM, "end"),
    ("R3", COL_RIGHT, RAIL_TOP, MIDDLE, "start"),
    ("R4", COL_RIGHT, MIDDLE, RAIL_BOTTOM, "start"),
]


class Schematic:
    """The bridge as a symbol: what is joined to what, and nothing further."""

    def type_name(self):
        return "A Wheatstone Bridge"

    def draw(self, ctx):
        o = ctx.constraints.pos

        # The two rails, and the source across them.
        _line(ctx, o, [(COL_LEFT, RAIL_TOP), (COL_RIGHT, RAIL_TOP)])
        _line(ctx, o, [(COL_LEFT, RAIL_BOTTOM), (COL_RIGHT, RAIL_BOTTOM)])
        self._source(ctx, o)
        self._ground(ctx, o)

        # The four arms, each a resistor between a rail and a midpoint.
        for (name, x, y0, y1, side) in ARMS:
            _line(ctx, o, _zigzag(x, y0, x, y1))
            edge = x - LABEL_GAP if side == "end" else x + LABEL_GAP
            _text(ctx, o, name, edge, (y0 + y1) / 2.0, anchor=side, middle_y=True)

        # The detector, bridging the two midpoints. Its leads stop at the face
        # rather than running under it: nothing here is painted over anything,
        # because there is no background to hide a line against.
        _line(ctx, o, [(COL_LEFT, MIDDLE), (CENTRE - METER_R, MIDDLE)])
        _line(ctx, o, [(CENTRE + METER_R, MIDDLE), (COL_RIGHT, MIDDLE)])
        _ring(ctx, o, CENTRE, MIDDLE, METER_R)
        _text(ctx, o, "G", CENTRE, MIDDLE, anchor="middle", middle_y=True, size=12.0)

        # The two midpoints: the only junctions in the circuit that are neither
        # driven nor grounded, and so the only two worth marking.
        for x in (COL_LEFT, COL_RIGHT):
            _dot(ctx, o, x, MIDDLE, 3.4)

        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(o, dex.Vector.new(WIDTH, HEIGHT))
        )

    def _source(self, ctx, o):
        """A battery on a branch of its own, joining the two rails.

        It has to *join* them. A source drawn on a stub off one rail, going up
        to nothing, is a branch no current can flow down — which is the one
        thing a schematic exists to rule out. So this runs out of the top rail,
        down the outside past two cells, and back into the bottom rail, and the
        loop the bridge sits inside is a loop you can trace with a finger.
        """
        _line(ctx, o, [(COL_LEFT, RAIL_TOP), (SOURCE_X, RAIL_TOP),
                       (SOURCE_X, MIDDLE - 16.0)])
        _line(ctx, o, [(SOURCE_X, MIDDLE + 16.0), (SOURCE_X, RAIL_BOTTOM),
                       (COL_LEFT, RAIL_BOTTOM)])
        # Two cells, each a long thin plate and a short thick one. Alternating
        # them is what says "a battery" rather than "a capacitor", and the long
        # plate is the positive terminal.
        for (k, y) in enumerate((MIDDLE - 9.0, MIDDLE - 3.0, MIDDLE + 3.0, MIDDLE + 9.0)):
            long_plate = k % 2 == 0
            half = 12.0 if long_plate else 6.0
            _line(ctx, o, [(SOURCE_X - half, y), (SOURCE_X + half, y)],
                  width=STROKE if long_plate else STROKE * 1.7)

    def _ground(self, ctx, o):
        """Three bars, shortening: the return the source is measured against."""
        _line(ctx, o, [(CENTRE, RAIL_BOTTOM), (CENTRE, RAIL_BOTTOM + 20.0)])
        for (k, half) in enumerate((15.0, 9.0, 4.0)):
            y = RAIL_BOTTOM + 20.0 + k * 5.0
            _line(ctx, o, [(CENTRE - half, y), (CENTRE + half, y)])


def transform():
    """The symbol. It takes nothing, because a kind of circuit has no values."""
    return Schematic()
