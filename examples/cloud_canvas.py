"""A canvas with a sky behind it, and a couple of things standing on the sky.

A transform can return a whole *surface* rather than a value, and a surface is
what a desktop shows — so a canvas a script builds can be lifted onto a tab of
its own. Point the lens at this lambda's result and choose **Clone to → New
desktop**, and the sky becomes somewhere you work.

The sky is a background member, which is what makes it a *backdrop* rather than
scenery: a canvas draws its background in its own frame, across the whole
viewport, so it stays put while everything on the surface pans and zooms over
it. Drag the canvas about and the sky does not move — which is what you want
from a sky and would be wrong for a gridline.

The clouds are cirrus rather than the cartoon kind: long tapered streaks laid
in loose bands, each faint enough that what you notice is where several of them
overlap. Each is drawn as two halves meeting on its centre line, one fading
upwards and one down, so both of its edges go soft — a single band can only ramp
one way, and the edge it does not fade is an edge, which makes a shape rather
than weather.

Every id is minted here rather than read back: the action queue does not drain
until this returns, so nothing built along the way can be looked up before then.
"""

import math

# Light, and blue enough to be a sky rather than a wall. Deeper overhead than
# along the bottom, as a real one is.
SKY_HIGH = (108, 168, 226)
SKY_LOW = (188, 220, 244)
CLOUD = (255, 255, 255)

# Bands of cirrus, as fractions of the viewport: (y, how far it sweeps, length,
# thickness, how solid, how many streaks in the band).
BANDS = (
    (0.08, 0.022, 0.70, 0.036, 0.34, 5),
    (0.19, -0.016, 0.30, 0.020, 0.26, 5),
    (0.31, 0.028, 0.84, 0.046, 0.40, 5),
    (0.44, -0.008, 0.26, 0.017, 0.24, 6),
    (0.55, 0.020, 0.64, 0.032, 0.32, 5),
    (0.67, -0.012, 0.36, 0.021, 0.26, 4),
    (0.78, 0.016, 0.76, 0.040, 0.34, 5),
    (0.91, 0.010, 0.32, 0.018, 0.22, 5),
)


def _v(x, y):
    return dex.Vector.new(x, y)


def _rgba(colour, alpha):
    return dex.Color.rgba(int(colour[0]), int(colour[1]), int(colour[2]), int(alpha))


def _solid(points, colour, alpha=255):
    return dex.Path.polygon(points, _rgba(colour, alpha), dex.Stroke.none())


def _ramp(points, near, far, angle, near_alpha, far_alpha=0):
    """A polygon whose interior runs from `near` to `far` along `angle`.

    Degrees clockwise from due right, so 270 runs bottom to top.
    """
    path = dex.Path.polygon(points, _rgba(near, near_alpha), dex.Stroke.none())
    path.fill_mode = dex.FillMode.linear()
    path.fill_end = _rgba(far, far_alpha)
    path.fill_angle = angle
    return path


def _drift(seed, salt=0):
    """A settled 0..1: the same sky every time it is opened."""
    x = (seed * 1103515245 + salt * 97 + 12345) & 0x7FFFFFFF
    x ^= x >> 13
    x = (x * 1274126177) & 0x7FFFFFFF
    return ((x >> 7) % 100003) / 100003.0


def _streak_half(x, y, length, thickness, tilt, seed, side):
    """One side of a streak: its centre line out to its soft edge.

    Drawn as two halves so that both edges fade. A single band can only ramp
    one way, which leaves the other edge cut off square — and a cloud with an
    edge is a shape rather than weather.

    Each half is pinched to nothing at both ends, so a streak fades out along
    its length as well as across it. The thickness wanders as it goes, which is
    what keeps a row of these from reading as a row of lenses.
    """
    steps = 26
    phase = _drift(seed, 3) * math.tau
    centre, outer = [], []
    for i in range(steps + 1):
        u = i / steps
        taper = math.sin(math.pi * u) ** 0.85
        ripple = 0.58 + 0.42 * math.sin(u * 6.4 + phase)
        half = thickness * taper * ripple
        px = x + length * u
        # Sagging along its own length, so a streak sweeps rather than rules.
        py = y + tilt * length * u + math.sin(u * 2.4 + phase) * thickness * 1.6
        centre.append(_v(px, py))
        outer.append(_v(px, py + side * half))
    outer.reverse()
    return centre + outer


class Sky:
    """The backdrop: a pale blue with cirrus drawn across it.

    A background member is handed the whole viewport, in the surface's own
    coordinates, so this paints across whatever it is given and never has to
    know where the plane over it has been dragged to.
    """

    def draw(self, ctx):
        origin = ctx.constraints.pos
        avail = ctx.constraints.available()
        w, h = avail.x, avail.y

        box = dex.DrawConstraints(
            pos=origin,
            x=dex.AxisConstraint.Exactly(w),
            y=dex.AxisConstraint.Exactly(h),
            wrap=None,
            should_clip=True,
        )

        def at(x, y):
            return _v(origin.x + x, origin.y + y)

        # The sky: deeper overhead, paler towards the bottom.
        ctx.draw_node(
            _ramp(
                [at(0.0, 0.0), at(w, 0.0), at(w, h), at(0.0, h)],
                SKY_LOW,
                SKY_HIGH,
                270.0,
                255,
                255,
            ),
            box,
        )

        # Cirrus, band by band. Each streak fades from its own underside
        # upwards, so a band has a soft edge without anything being blurred.
        seed = 0
        for by, tilt, length, thickness, weight, count in BANDS:
            for i in range(count):
                seed += 1
                start = -0.12 + (i + _drift(seed, 1) * 0.7) / count * 1.24
                jitter = (_drift(seed, 2) - 0.5) * 0.05
                span = length * (0.6 + 0.7 * _drift(seed, 4))
                alpha = 255 * weight * (0.55 + 0.45 * _drift(seed, 5))
                # Above the centre line fading upwards, below it fading down.
                def wisp(length, width, weight, seed=seed, start=start, jitter=jitter):
                    for side, angle in ((-1.0, 270.0), (1.0, 90.0)):
                        ctx.draw_node(
                            _ramp(
                                _streak_half(
                                    origin.x + w * start,
                                    origin.y + h * (by + jitter),
                                    length,
                                    width,
                                    tilt,
                                    seed,
                                    side,
                                ),
                                CLOUD,
                                CLOUD,
                                angle,
                                weight,
                                0,
                            ),
                            box,
                        )

                wisp(w * span, h * thickness, alpha)
                # Some streaks carry a second, shorter one along the same
                # line. Density comes from streaks piling up, never from
                # drawing a thin bright one: below a couple of points across a
                # tapered shape stops being soft and starts being a dash.
                if _drift(seed, 6) > 0.45:
                    wisp(w * span * 0.62, h * thickness * 0.62, alpha * 0.9)

        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(origin, dex.Vector.new(w, h))
        )

    def type_name(self):
        return "A Sky"


def build(ws):
    """A canvas with the sky behind it; returns its uid."""
    canvas = dex.Canvas.build(ws)
    # Named, so the lens, the breadcrumb trail and the tab all call it what it
    # is rather than "A Canvas".
    ws.submit_action(canvas, dex.NameCanvas("Sky"), "Named the surface")

    sky = dex.NodeUid.mint()
    ws.insert_node_at_dyn(sky, Sky())
    ws.submit_action(canvas, dex.AdoptCanvasNode(sky, dex.Layer.background()), "Hung the sky")

    # A couple of things standing on it, so there is something to drag about
    # and watch the sky stay put behind.
    for i, (label, x, y) in enumerate(
        (("drag me", 140.0, 200.0), ("the sky stays put", 320.0, 300.0))
    ):
        child = dex.NodeUid.mint()
        ws.insert_node_at_dyn(child, dex.LabelEditable.new(label))
        item = dex.CanvasNode.build(ws, child, dex.Vector.new(x, y), dex.Vector.new(170.0, 30.0))
        ws.submit_action(
            canvas, dex.AdoptCanvasNode(item, dex.Layer.midground()), f"Placed {i}"
        )

    return canvas


def transform():
    """A lambda returning a canvas you can take to a desktop of its own."""
    return build(dex.ws)
