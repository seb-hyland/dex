"""A campfire: a node whose drawing advances by itself, frame after frame.

Nothing in dex has to be told to animate. The application asks for the next
frame the moment it finishes one, so a node's `draw` runs continuously, and a
drawing that reads the clock is a drawing that moves. There is no frame counter
to subscribe to and no timer to install — `time.monotonic()` is the whole of it.

That is the point worth taking from this file. The rest is a demonstration, in
flat polygons: three logs, a bed of embers, six flame tongues that lick and
breathe past each other, flame torn loose and drifting up, sparks that rise and
go out, and the light all of it throws. The fire is drawn with gradient fills,
which is what keeps a flat-shaded flame from reading as a paper cut-out — a
tongue that fades to nothing at its tip looks like flame, and one that stops
dead looks like a leaf.

Paste it into a lambda's editor and press Run.
"""

import math
import time

TAU = math.tau

# The node's own box. Everything below is in these coordinates, with (0, 0) at
# the top-left, and is offset to wherever the node is drawn at the last moment.
WIDTH = 320.0
HEIGHT = 340.0

# Where the fire sits, and how far the sparks get.
HEARTH_X = WIDTH * 0.5
HEARTH_Y = 262.0
SPARK_CEILING = 18.0

# -- the palette ---------------------------------------------------------
#
# A night scene wants few colours and a lot of contrast between them: two for
# the dark, three for the wood, and a ramp from red through orange to almost
# white for the fire itself.

NIGHT = (17, 16, 28)
NIGHT_FAR = (28, 26, 44)
GROUND = (10, 9, 16)

LOG_DARK = (36, 24, 23)
LOG_BODY = (50, 33, 27)
LOG_LIT = (98, 65, 42)
LOG_CAP = (104, 71, 47)

EMBER_LOW = (168, 54, 26)
EMBER_HIGH = (255, 178, 76)

GLOW = (255, 146, 58)
GLOW_CORE = (255, 196, 110)
FLAME_OUTER = (206, 62, 34)
FLAME_OUTER_TIP = (128, 30, 28)
FLAME_MID = (240, 116, 38)
FLAME_MID_TIP = (206, 58, 32)
FLAME_INNER = (250, 178, 62)
FLAME_INNER_TIP = (240, 108, 40)
FLAME_CORE = (255, 244, 208)
FLAME_CORE_TIP = (253, 194, 92)
SPARK_HOT = (255, 226, 152)
SPARK_COOL = (222, 84, 36)


def rgb(c, alpha=255):
    return dex.Color.rgba(int(c[0]), int(c[1]), int(c[2]), int(alpha))


def blend(a, b, t):
    """`a` at 0, `b` at 1."""
    t = min(1.0, max(0.0, t))
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


# -- pseudo-randomness ---------------------------------------------------
#
# Deterministic, so a spark is the same spark on every frame and the fire looks
# the same each time it is opened. `random` would give a different scatter every
# run and a different one again after a save.


def _rand(seed, salt=0):
    x = (seed * 1103515245 + salt * 97 + 12345) & 0x7FFFFFFF
    x ^= x >> 13
    x = (x * 1274126177) & 0x7FFFFFFF
    return ((x >> 7) % 100003) / 100003.0


# -- geometry helpers ----------------------------------------------------


def _v(x, y):
    return dex.Vector.new(x, y)


def _disc(cx, cy, rx, ry, sides=24):
    """A closed ring of `sides` points around `(cx, cy)`.

    Kept low. A radial fill is chased with subdivision until it reads as smooth,
    so every side of the outline it starts from is paid for many times over —
    and on a glow whose edge is transparent anyway, sides past a dozen or so buy
    nothing at all.
    """
    return [
        _v(cx + rx * math.cos(TAU * i / sides), cy + ry * math.sin(TAU * i / sides))
        for i in range(sides)
    ]


def _solid(points, colour, alpha=255):
    return dex.Path.polygon(points, rgb(colour, alpha), dex.Stroke.none())


def _ramp(points, near, far, angle, near_alpha=255, far_alpha=0):
    """A polygon whose interior runs from `near` to `far` along `angle`.

    Degrees are clockwise from due right, so 270 runs bottom to top — which is
    how a flame is lit, and how the glow falls away from the ground.
    """
    path = dex.Path.polygon(points, rgb(near, near_alpha), dex.Stroke.none())
    path.fill_mode = dex.FillMode.linear()
    path.fill_end = rgb(far, far_alpha)
    path.fill_angle = angle
    return path


def _halo(points, near, far, near_alpha=255, far_alpha=0):
    """The same, running out from the middle: a point of light."""
    path = dex.Path.polygon(points, rgb(near, near_alpha), dex.Stroke.none())
    path.fill_mode = dex.FillMode.radial()
    path.fill_end = rgb(far, far_alpha)
    return path


# -- the fire's own arithmetic -------------------------------------------


def flicker(t, phase=0.0):
    """A restless 0..1, from three waves that never quite line up again.

    One sine is a pulse and reads as breathing; three at incommensurate rates
    read as a fire, which is never quite periodic.
    """
    v = (
        0.55 * math.sin(t * 3.1 + phase)
        + 0.30 * math.sin(t * 7.7 + phase * 1.7 + 1.3)
        + 0.15 * math.sin(t * 15.3 + phase * 0.4 + 2.9)
    )
    return 0.5 + 0.5 * v


def tongue(cx, base_y, half_width, height, lean, wobble, t, speed, phase, steps=11):
    """One flame tongue: a closed outline up one side and down the other.

    The width follows `sqrt(1 - u)`, which is fat at the base and comes to a
    point — the shape a flame actually takes. The sway is multiplied by `u`
    raised above one, so the foot stays planted on the log and only the tip
    whips about.
    """
    left, right = [], []
    for i in range(steps + 1):
        u = i / steps
        # Widest a third of the way up, and pointed at the top: a flame is a
        # bulge with a tail, not a triangle.
        bulge = math.sin(math.pi * (0.30 + 0.70 * u))
        w = half_width * bulge * (1.0 - u) ** 0.30
        # Two waves rather than one, so an edge ripples instead of swinging.
        sway = wobble * (
            math.sin(u * 3.4 + t * speed + phase) * (u ** 1.4)
            + 0.38 * math.sin(u * 7.9 + t * speed * 1.7 + phase * 2.1) * (u ** 2.0)
        )
        x = cx + lean * (u ** 1.3) + sway
        y = base_y - height * u
        left.append(_v(x - w, y))
        right.append(_v(x + w, y))
    right.reverse()
    return left + right


def teardrop(cx, cy, half_width, height, t, speed, phase, steps=9):
    """A closed shape narrow at both ends, for flame with nothing under it.

    A tongue has a flat foot, which is right when it is standing on a log and
    quite wrong in mid-air: it reads as a box. This is pinched top and bottom,
    fattest low down, the way a piece of flame that has torn loose actually
    goes.
    """
    left, right = [], []
    for i in range(steps + 1):
        u = i / steps
        w = half_width * math.sin(math.pi * u) ** 0.48 * (1.0 - 0.45 * u)
        sway = 0.30 * half_width * math.sin(u * 4.1 + t * speed + phase) * u
        x = cx + sway
        y = cy - height * u
        left.append(_v(x - w, y))
        right.append(_v(x + w, y))
    right.reverse()
    return left + right


# -- the scene -----------------------------------------------------------


def _backdrop():
    """Night, with the dark lifting a little towards the fire."""
    whole = [_v(0.0, 0.0), _v(WIDTH, 0.0), _v(WIDTH, HEIGHT), _v(0.0, HEIGHT)]
    return _ramp(whole, NIGHT_FAR, NIGHT, 90.0, 255, 255)


def _ground():
    """The shadow the logs sit in, which is what gives them any weight.

    Soft-edged for the same reason the glow is: a flat ellipse under a lit
    clearing reads as a hole cut in it. Round, so the ramp reaches nothing
    exactly where the shape ends.
    """
    return _halo(
        _disc(HEARTH_X, HEARTH_Y + 30.0, 104.0, 104.0, 14),
        GROUND,
        GROUND,
        near_alpha=150,
        far_alpha=0,
    )


def _logs():
    """Three logs: one across the back, two crossed in front of it.

    Each is four flat shapes — a body, a narrow lit face along its upper edge,
    a shadow along its lower one, and an end cap — which is the whole of the
    trick. Firelight comes from one place, so a thin strip of a round log is
    lit and the rest of it is turned away.
    """
    shapes = []
    for cx, cy, half_len, radius, angle in (
        (HEARTH_X + 4.0, HEARTH_Y + 20.0, 96.0, 15.0, 0.06),
        (HEARTH_X - 26.0, HEARTH_Y + 34.0, 84.0, 13.0, -0.30),
        (HEARTH_X + 30.0, HEARTH_Y + 36.0, 80.0, 12.0, 0.26),
    ):
        ax, ay = math.cos(angle), math.sin(angle)
        px, py = -ay, ax

        def at(along, across, ax=ax, ay=ay, px=px, py=py, cx=cx, cy=cy):
            return _v(cx + ax * along + px * across, cy + ay * along + py * across)

        shapes.append(
            _solid(
                [
                    at(-half_len, -radius),
                    at(half_len, -radius),
                    at(half_len, radius),
                    at(-half_len, radius),
                ],
                LOG_BODY,
            )
        )
        # The upper third catches the light; the rest of the round is in shadow.
        shapes.append(
            _solid(
                [
                    at(-half_len, -radius),
                    at(half_len, -radius),
                    at(half_len, -radius * 0.52),
                    at(-half_len, -radius * 0.52),
                ],
                LOG_LIT,
            )
        )
        shapes.append(
            _solid(
                [
                    at(-half_len, radius),
                    at(half_len, radius),
                    at(half_len, radius * 0.35),
                    at(-half_len, radius * 0.35),
                ],
                LOG_DARK,
            )
        )
        # The cut end, facing the viewer.
        end_x, end_y = cx + ax * half_len, cy + ay * half_len
        shapes.append(_solid(_disc(end_x, end_y, radius * 0.34, radius, 12), LOG_CAP))
    return shapes


def _embers(t):
    """The bed the flames stand in, each coal breathing on its own count."""
    shapes = []
    for i in range(7):
        heat = flicker(t, phase=i * 1.7)
        x = HEARTH_X + (_rand(i, 11) - 0.5) * 132.0
        y = HEARTH_Y + 10.0 + (_rand(i, 12) - 0.5) * 22.0
        size = 4.0 + _rand(i, 13) * 7.0
        colour = blend(EMBER_LOW, EMBER_HIGH, heat)
        shapes.append(
            _halo(
                _disc(x, y, size * 1.9, size * 1.2, 8),
                colour,
                EMBER_LOW,
                near_alpha=int(150 + 105 * heat),
                far_alpha=0,
            )
        )
    return shapes


def _wash(t):
    """The light the fire throws into the night, pulsing with it.

    Round, and larger than the box, which the scene is clipped to. A radial ramp
    runs from the middle of a shape to its furthest corner, so on anything but a
    circle it is still partly lit where the outline is — and that shows as the
    edge of an ellipse hanging in the dark. On a circle it reaches nothing
    exactly where the shape ends, which is the only way for a glow to have no
    edge at all.
    """
    beat = flicker(t, phase=0.4)
    radius = 186.0 + 26.0 * beat
    return _halo(
        _disc(HEARTH_X, HEARTH_Y - 16.0, radius, radius, 16),
        GLOW,
        GLOW,
        near_alpha=int(104 + 40 * beat),
        far_alpha=0,
    )


def _hearth_light(t):
    """The near light, drawn over the logs so that it lands on them.

    Without this the wood is lit from nowhere: the fire is standing on it and
    the two never meet.
    """
    beat = flicker(t, phase=2.2)
    radius = 78.0 + 12.0 * beat
    return _halo(
        _disc(HEARTH_X, HEARTH_Y + 4.0, radius, radius, 14),
        GLOW_CORE,
        GLOW,
        near_alpha=int(64 + 30 * beat),
        far_alpha=0,
    )


# (x offset, half-width, height, lean, wobble, speed, phase, near, far, alpha)
#
# The body is one broad, low tongue with three narrower ones standing in it at
# different offsets and different rates. Nested tongues on one axis read as a
# single cone with a highlight down the middle; offset ones read as fire.
FLAMES = (
    (0.0, 54.0, 132.0, -4.0, 13.0, 2.10, 0.0, FLAME_OUTER, FLAME_OUTER_TIP, 215),
    (-23.0, 22.0, 152.0, 13.0, 16.0, 2.90, 1.9, FLAME_OUTER, FLAME_OUTER_TIP, 205),
    (19.0, 24.0, 162.0, -11.0, 15.0, 2.55, 4.1, FLAME_OUTER, FLAME_OUTER_TIP, 205),
    (-4.0, 26.0, 122.0, 5.0, 11.0, 3.35, 2.7, FLAME_MID, FLAME_MID_TIP, 235),
    (6.0, 15.0, 96.0, -4.0, 9.0, 4.05, 3.6, FLAME_INNER, FLAME_INNER_TIP, 245),
    (-6.0, 13.0, 40.0, 3.0, 5.0, 5.30, 5.2, FLAME_CORE, FLAME_CORE_TIP, 255),
)

# Smaller tongues off to the sides, licking up between the logs.
LICKS = (
    (-48.0, 12.0, 66.0, -10.0, 10.0, 4.10, 2.4),
    (44.0, 11.0, 58.0, 9.0, 9.0, 4.70, 0.8),
    (-30.0, 9.0, 44.0, 6.0, 8.0, 5.60, 4.3),
)

# Torn-off flame, riding up above the fire and going out. (x, width, height,
# how far it climbs, how long it takes, when it starts.)
WISPS = (
    (-12.0, 6.5, 34.0, 82.0, 1.9, 0.0),
    (10.0, 5.0, 26.0, 96.0, 2.4, 0.9),
)


def _flames(t):
    shapes = []
    breath = 0.86 + 0.24 * flicker(t, phase=1.1)
    for dx, half, height, lean, wobble, speed, phase, near, far, alpha in FLAMES:
        # Each tongue breathes on its own count as well as with the fire, so
        # they never all reach for the top at once.
        own = 0.80 + 0.34 * flicker(t, phase=phase * 1.6)
        shapes.append(
            _ramp(
                tongue(
                    HEARTH_X + dx,
                    HEARTH_Y + 2.0,
                    half,
                    height * breath * own,
                    lean,
                    wobble,
                    t,
                    speed,
                    phase,
                ),
                near,
                far,
                270.0,
                near_alpha=alpha,
                far_alpha=0,
            )
        )
    for dx, half, height, lean, wobble, speed, phase in LICKS:
        lick = 0.45 + 0.75 * flicker(t, phase=phase * 2.0)
        shapes.append(
            _ramp(
                tongue(
                    HEARTH_X + dx,
                    HEARTH_Y + 10.0,
                    half,
                    height * lick,
                    lean,
                    wobble,
                    t,
                    speed,
                    phase,
                    steps=8,
                ),
                FLAME_MID,
                FLAME_MID_TIP,
                270.0,
                near_alpha=190,
                far_alpha=0,
            )
        )
    return shapes


def _wisps(t):
    """Flame that has torn loose and is on its way out."""
    shapes = []
    for i, (dx, half, height, climb, life, offset) in enumerate(WISPS):
        u = ((t / life) + offset) % 1.0
        # Slowing as it goes, shrinking, and gone before it leaves the frame.
        rise = climb * (1.0 - (1.0 - u) ** 2.0)
        scale = 1.0 - 0.65 * u
        fade = math.sin(math.pi * min(1.0, u * 1.25)) ** 0.8
        shapes.append(
            _ramp(
                teardrop(
                    HEARTH_X + dx + math.sin(u * 4.0 + i) * 13.0,
                    HEARTH_Y - 96.0 - rise,
                    half * scale,
                    height * scale,
                    t,
                    4.4,
                    offset * 3.0,
                ),
                FLAME_MID,
                FLAME_OUTER_TIP,
                270.0,
                near_alpha=int(215 * fade),
                far_alpha=0,
            )
        )
    return shapes


SPARKS = 46


def _sparks(t):
    """Embers carried up on the draught, cooling and going out.

    Each has its own lifetime and its own place in it, so they do not pulse
    together — which is the thing that gives a loop away. They leave the fire in
    a narrow column and spread as they climb, because that is what the draught
    above a fire does.
    """
    shapes = []
    for i in range(SPARKS):
        life = 1.5 + _rand(i, 1) * 2.4
        u = ((t / life) + _rand(i, 2)) % 1.0
        rise = (HEARTH_Y - SPARK_CEILING) * (0.40 + 0.60 * _rand(i, 3))
        # Fast off the fire, slowing as they go.
        climb = 1.0 - (1.0 - u) ** 1.8
        x = HEARTH_X + (_rand(i, 4) - 0.5) * 46.0
        x += math.sin(climb * 4.6 + _rand(i, 5) * TAU) * (14.0 + 30.0 * _rand(i, 6)) * climb
        y = HEARTH_Y - 10.0 - rise * climb
        # Mostly small; a few bigger ones to catch the eye.
        weight = _rand(i, 7) ** 2.2
        size = (0.8 + weight * 2.2) * (1.0 - 0.55 * climb)
        # Lit on the way out, gone well before the top.
        fade = math.sin(math.pi * min(1.0, u * 1.2)) ** 0.55
        colour = blend(SPARK_HOT, SPARK_COOL, climb ** 0.6)
        shapes.append(
            _solid(
                [
                    _v(x, y - size * 2.4),
                    _v(x + size, y),
                    _v(x, y + size * 2.4),
                    _v(x - size, y),
                ],
                colour,
                alpha=int(240 * fade),
            )
        )
    return shapes


class Campfire:
    """A fire, drawn from the back of the scene to the front."""

    def __init__(self):
        # When this node was made, so the fire starts where it is placed rather
        # than somewhere in the middle of the process's lifetime.
        self.started = time.monotonic()

    # `draw` runs every frame, so reading the clock here is all it takes.
    def draw(self, ctx):
        origin = ctx.constraints.pos
        t = time.monotonic() - self.started

        # Everything is built in the node's own coordinates and placed by the
        # constraints, so the scene does not have to know where it ended up.
        # Clipped to the node's own box. The glow is drawn larger than the
        # scene on purpose — that is how it has no edge — so something has to
        # stop it, and a node spilling over its neighbours is not it.
        box = dex.DrawConstraints(
            pos=origin,
            x=dex.AxisConstraint.Exactly(WIDTH),
            y=dex.AxisConstraint.Exactly(HEIGHT),
            wrap=None,
            should_clip=True,
        )
        # Back to front: the night, the light it is thrown into, the ground,
        # the wood, the light landing *on* the wood, and then the fire itself.
        for shape in (
            [_backdrop(), _wash(t), _ground()]
            + _logs()
            + [_hearth_light(t)]
            + _embers(t)
            + _flames(t)
            + _wisps(t)
            + _sparks(t)
        ):
            ctx.draw_node(shape, box)

        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(origin, dex.Vector.new(WIDTH, HEIGHT))
        )

    def type_name(self):
        return "A Campfire"


def transform():
    return Campfire()
