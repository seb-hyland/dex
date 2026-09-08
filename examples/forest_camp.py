"""A canvas that is somewhere: a night forest behind the plane, a fire in front.

A surface paints in three bands — a background, the items on the plane, and a
foreground — and the two outer ones are both handed the whole viewport in the
surface's own coordinates. Neither of them moves when the plane does. So a scene
can be built out of the pair: the forest goes in the background, the campfire in
the foreground, and the plane runs between them. Drag the canvas about and the
camp stays exactly where it is, while everything you have put on the surface
slides between the trees and the firelight.

That is the whole of the arrangement. The rest is two drawings.

The forest is mostly still, and the still part is built once for a given
viewport and redrawn from the list it built — the trees cost their trigonometry
once rather than sixty times a second. What does move is drawn fresh every
frame: the stars breathe, and the fireflies wander the treeline with a trail of
their own light behind them, some of them out in front of the wood and some
back among it. The cache is the seam between the two, and it is worth having
only because the seam is real.

The fire is `campfire.py`, with two changes: its own night backdrop is gone,
because the forest is the night now, and its hearth is pinned to the treeline
rather than to a fixed box, so the camp composes itself at whatever size the
window is. Taking the backdrop out is what lets the fire light the forest: with
nothing opaque under it, the glow it throws falls on the trees.

Paste it into a lambda's editor and press Run. Point the lens at the result and
choose **Clone to → New desktop**, and the camp becomes somewhere you work.
"""

import math
import time

TAU = math.tau


# =======================================================================
# Shared drawing
# =======================================================================


def rgb(c, alpha=255):
    return dex.Color.rgba(int(c[0]), int(c[1]), int(c[2]), int(alpha))


def blend(a, b, t):
    """`a` at 0, `b` at 1."""
    t = min(1.0, max(0.0, t))
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def _v(x, y):
    return dex.Vector.new(x, y)


def _rand(seed, salt=0):
    """Deterministic 0..1: the same forest, and the same sparks, every time.

    `random` would scatter the trees differently on every run and differently
    again after a save, which is no way to keep a place.
    """
    x = (seed * 1103515245 + salt * 97 + 12345) & 0x7FFFFFFF
    x ^= x >> 13
    x = (x * 1274126177) & 0x7FFFFFFF
    return ((x >> 7) % 100003) / 100003.0


def _disc(cx, cy, rx, ry, sides=24):
    """A closed ring of `sides` points around `(cx, cy)`.

    Kept low. A radial fill is chased with subdivision until it reads as smooth,
    so every side of the outline it starts from is paid for many times over.
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
    how a flame is lit, and how a sky deepens away from the horizon.
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


# =======================================================================
# The forest
# =======================================================================
#
# Flat silhouettes, in four ranges. Nothing here is lit: a night wood is read
# entirely from how dark each range is against the one behind it, so the whole
# forest is four colours, a little haze between them, and the distance that
# makes.

SKY_HIGH = (11, 13, 26)
SKY_LOW = (44, 50, 78)

STAR = (228, 234, 250)
STAR_COOL = (176, 198, 240)

MOON = (246, 246, 232)
MOON_DIM = (196, 200, 206)
MOON_HALO = (128, 150, 196)

# Night cloud is not white. It is the sky a shade lighter where the moon is
# behind it and a shade darker where it is not, and that is the whole of it.
CLOUD = (52, 61, 92)
CLOUD_LIT = (142, 160, 200)

# Four flat tones, each range a step darker than the one behind it. The steps
# are what the eye reads as distance; nothing else here says how far away
# anything is.
TREE_DEEP = (40, 48, 75)
TREE_FAR = (30, 37, 58)
TREE_MID = (15, 19, 32)
TREE_NEAR = (5, 6, 12)
MIST = (78, 92, 124)

GROUND_FAR = (17, 20, 31)
GROUND_NEAR = (5, 5, 10)

FIREFLY = (236, 214, 104)
FIREFLY_CORE = (255, 248, 176)

# Where the far trees stand, as a fraction of the height, and how far the ground
# wanders either side of it. A ruled horizon reads as a table edge.
HORIZON = 0.64
GROUND_ROLL = 0.024


def _ground_y(w, h, x):
    """The ground line at `x`: two slow waves that never repeat together."""
    u = x / max(w, 1.0)
    roll = 0.62 * math.sin(u * 5.1 + 0.7) + 0.38 * math.sin(u * 11.3 + 1.9)
    return h * HORIZON + h * GROUND_ROLL * roll


def _canopy(u):
    """How tall the wood is at `u` across the width, and why it is not even.

    Trees that only vary tree by tree give an even band with a ragged top edge,
    which is a hedge. A wood has *swells* — a stand of tall ones here, a dip
    there — and they are much wider than any one tree, so they cannot come out
    of the same roll the trees do. Three slow waves across the frame, and every
    tree in every range multiplies its height by whatever this says where it
    stands.

    The second term is the clearing. Something has to explain why the camp is
    where it is, and a fire in a wood is in the one place the wood is not: the
    canopy lies down through the middle of the frame, which both says *clearing*
    and keeps a hundred-foot fir from growing straight up out of the fire.
    """
    swell = (
        0.54 * math.sin(u * 2.3 + 0.6)
        + 0.31 * math.sin(u * 5.7 + 2.4)
        + 0.15 * math.sin(u * 9.1 + 4.1)
    )
    clearing = 1.0 - 0.54 * math.exp(-(((u - 0.5) / 0.17) ** 2))
    return (1.0 + 0.44 * swell) * clearing


def _fir(x, base, height, half, tiers, seed, trunk_frac=0.12, droop=1.0):
    """One fir, as a single closed outline: up one side and down the other.

    A conifer is not a sawtooth triangle. Its branches spring from the trunk and
    then *fall*, so every one of them is a wedge pointing out and down, and the
    line between two of them runs up and inward rather than straight across.
    Drawing the tip below where the branch is attached is the whole difference
    between a fir and a Christmas tree, and it costs one number.

    Three more things keep a stand of these from reading as a row of stamps.
    The two sides are drawn from separate rolls, so no tree is symmetric. The
    trunk carries a lean that gathers with height, so it curves rather than
    rules. And the reach falls off faster than the height climbs, which is what
    leaves the thin bare spire at the top that says conifer from a mile off.

    The caller decides the rest. `tiers` few and `half` small is a spindly snag;
    `trunk_frac` large is an old tree with its lower branches long gone.
    """
    lean = (_rand(seed, 21) - 0.5) * 0.17
    # Some trees are full and some are spindly. One roll, applied to every
    # branch, so a tree is consistently one or the other.
    bushy = 0.76 + 0.48 * _rand(seed, 22)
    stem = max(0.8, half * 0.085)

    def spine(u):
        return x + lean * height * (u ** 1.5)

    def side(sign, salt):
        pts = []
        for i in range(tiers):
            u = trunk_frac + (1.0 - trunk_frac) * i / tiers
            y = base - height * u
            reach = half * bushy * (1.0 - u) ** 1.20 * (0.56 + 0.82 * _rand(seed, salt + i))
            fall = reach * droop * (0.28 + 0.36 * _rand(seed, salt + 40 + i))
            # Where it springs from the trunk, then the tip it falls to.
            pts.append(_v(spine(u) + sign * stem * (1.0 - 0.45 * u), y))
            pts.append(_v(spine(u) + sign * reach, y + fall))
        return pts

    left = [_v(spine(0.0) - stem, base)] + side(-1.0, 50)
    right = [_v(spine(0.0) + stem, base)] + side(1.0, 120)
    right.reverse()
    return left + [_v(spine(1.0), base - height)] + right


# (how many, where they start and stop across the width, how far below the
# ground line they stand, height as a fraction of the viewport, spread of that
# height, how wide against their own height, tiers, how far the branches fall,
# colour, how much haze gathers at their feet).
RANGES = (
    (56, -0.05, 1.05, -0.012, 0.062, 0.044, 0.30, 6, 0.85, TREE_DEEP, 74),
    (42, -0.05, 1.05, 0.014, 0.100, 0.066, 0.31, 7, 0.95, TREE_FAR, 54),
    (28, -0.06, 1.06, 0.058, 0.138, 0.092, 0.33, 8, 1.05, TREE_MID, 34),
)

# The near range is drawn out at the edges only: the middle of the frame is the
# camp's, and a black fir standing in front of the fire would be the end of the
# scene rather than a frame around it.
NEAR_TREES = (
    (-0.060, 0.54),
    (0.028, 0.44),
    (0.086, 0.70),
    (0.158, 0.48),
    (0.214, 0.38),
    (0.262, 0.31),
    (0.742, 0.33),
    (0.796, 0.42),
    (0.856, 0.52),
    (0.918, 0.38),
    (0.958, 0.66),
    (1.048, 0.50),
)

STARS = 96


def _sky(w, h):
    whole = [_v(0.0, 0.0), _v(w, 0.0), _v(w, h), _v(0.0, h)]
    return _ramp(whole, SKY_LOW, SKY_HIGH, 270.0, 255, 255)


def _lens(cx, cy, half_w, half_h, seed, steps=22):
    """The centre line of a long soft shape, and its half-thickness along it.

    Pinched to nothing at both ends and rippling as it goes, so a cloud is not
    an ellipse. Handed back as points rather than a polygon, because it is drawn
    twice: once fading up out of the line and once fading down.
    """
    phase = _rand(seed, 3) * TAU
    line = []
    for i in range(steps + 1):
        u = i / steps
        taper = math.sin(math.pi * u) ** 0.62
        ripple = 0.55 + 0.45 * math.sin(u * 5.1 + phase)
        x = cx - half_w + 2.0 * half_w * u
        # Sagging along its own length, so a bank drifts rather than rules.
        y = cy + math.sin(u * 2.2 + phase) * half_h * 0.9
        line.append((x, y, half_h * taper * ripple))
    return line


def _cloud(cx, cy, half_w, half_h, seed, colour, alpha, strands=5, spread=1.4):
    """A bank of night cloud: soft strands piled along one line.

    Drawn as strands rather than as one body, and that is the whole of it. A
    single soft lens the size of a cloud has a definite underside however gently
    it fades, and against a dark sky a long shape with an underside is a ridge —
    the eye reads it as land every time. Several of them at slightly different
    heights never resolve into an edge, because no two of them end in the same
    place — but keep them wispy: thin strands well scattered in height, not a
    stack of them at one level, or the piled edges line up into rings.

    Each is two halves meeting on its centre line, one ramping up and one down,
    because a band can only fade one way and the side it does not fade is the
    edge that made the ridge. Linear ramps, not radial fills, so a whole sky of
    them costs a fraction of what the same weather drawn as soft discs would.
    """
    shapes = []
    for k in range(strands):
        seed_k = seed * 13 + k
        line = _lens(
            cx + (_rand(seed_k, 72) - 0.5) * half_w * 0.34,
            cy + (_rand(seed_k, 71) - 0.5) * half_h * spread * 2.0,
            half_w * (0.66 + 0.40 * _rand(seed_k, 73)),
            half_h * (0.50 + 0.65 * _rand(seed_k, 74)),
            seed_k,
        )
        # Divided down by how many are piling up, so adding strands makes a
        # bank softer rather than simply darker.
        weight = alpha * (0.66 + 0.54 * _rand(seed_k, 75)) * (3.0 / strands)
        for side, angle, strength in ((-1.0, 270.0, weight), (1.0, 90.0, weight * 0.8)):
            near = [_v(x, y) for x, y, _t in line]
            far = [_v(x, y + side * t) for x, y, t in line]
            far.reverse()
            shapes.append(_ramp(near + far, colour, colour, angle, strength, 0))
    return shapes


def _moon(w, h):
    """A moon behind cloud, and the ombre it puts on the sky around it.

    One ramp cannot fall off the way light does — it is a straight line from
    here to there, and a moon lit that way has a visible end. Rings, each wider
    and fainter than the last, add up to something that keeps fading the whole
    way out, because the sum of five ramps is not a ramp.

    The disc is a ramp of its own, running across it rather than out from the
    middle: a flat white circle is a hole punched in the sky, and one that is
    brighter on the side it is lit from is a moon.

    Then the cloud goes over the top of all of it. A bank thin enough to see the
    disc through is worth more than a clear sky: it is the one thing in the
    frame that says the air has any depth to it, and it puts a lit edge in the
    sky for the fire's glow further down to answer.
    """
    cx, cy = w * 0.865, h * 0.135
    r = max(12.0, h * 0.030)
    # (how wide, how strong, how many sides). A radial fill is chased with
    # subdivision until the ramp across it reads as smooth, and the chase starts
    # from every side of the outline — so sides are the expensive number here,
    # not radius. The wide rings get few: a twenty-sided ring three hundred
    # pixels across is a pixel off round at its flattest, and it is fading to
    # nothing there anyway.
    rings = ((16.0, 22, 20), (9.0, 30, 20), (4.8, 40, 24), (2.4, 58, 28), (1.5, 86, 32))
    shapes = [
        _halo(_disc(cx, cy, r * scale, r * scale, sides), MOON_HALO, MOON_HALO, alpha, 0)
        for scale, alpha, sides in rings
    ]
    # 45 degrees is clockwise from due right, so the far end is down and to the
    # right and the light comes over the top-left shoulder.
    shapes.append(_ramp(_disc(cx, cy, r, r, 28), MOON, MOON_DIM, 45.0, 240, 190))

    # The bank across it: a few wispy strands spread wider than the disc and
    # well scattered in height, so some cross the moon and some pass above and
    # below — a moon seen *between* cloud, not one behind a curtain, and too few
    # and too scattered to stack into a ring.
    shapes += _cloud(cx - w * 0.04, cy, w * 0.20, r * 0.66, 7, CLOUD, 176, 4, 2.6)
    # And one lit strand, because a cloud in front of the moon is the brightest
    # thing in the sky rather than the darkest.
    shapes += _cloud(cx - w * 0.01, cy + r * 0.20, w * 0.11, r * 0.16, 8, CLOUD_LIT, 84, 2, 2.2)
    return shapes


# Cloud elsewhere in the sky: (across, down, half width, half height, how
# strong). Few and low-contrast — they are there to keep the sky from being a
# flat sheet with one moon on it, not to be looked at.
CLOUDS = (
    (0.14, 0.13, 0.30, 0.058, 96),
    (0.54, 0.22, 0.24, 0.044, 74),
    (0.78, 0.06, 0.20, 0.034, 62),
    (0.34, 0.34, 0.21, 0.026, 44),
)


def _clouds(w, h):
    shapes = []
    for i, (u, v, half_w, half_h, alpha) in enumerate(CLOUDS):
        shapes += _cloud(
            w * u, h * v, w * half_w, h * half_h, 20 + i, CLOUD, alpha, 5, 1.5
        )
    return shapes


def _sparkle(x, y, r, alpha, colour):
    """A four-armed star: eight points, alternating long and short.

    For the brightest few only. A radial fill from the middle runs out along the
    arms, so the spikes fade rather than ending — which is what an eye does with
    a bright point, and what a hard cross drawn over one never looks like.
    """
    pts = []
    for i in range(8):
        reach = r if i % 2 == 0 else r * 0.22
        a = TAU * i / 8.0
        pts.append(_v(x + reach * math.cos(a), y + reach * math.sin(a)))
    return _halo(pts, colour, colour, alpha, 0)


def _stars(w, h, t):
    """A hard point with a bloom around it, thinning out towards the trees.

    A star drawn as a soft blob is a smudge, and one drawn as a hard chip is a
    speck of dirt on the glass. It is the two together that read: a core one or
    two pixels across, sharp, with a faint halo three or four times its size.
    The halo alone is what was wrong before — all bloom and nothing to bloom
    from.

    They do not all twinkle at the same rate, and the slowest of them barely
    twinkle at all — a sky where every point pulses together is a string of
    fairy lights. The core is worked harder than the halo, because scintillation
    is the point moving in and out of visibility and not the air around it.
    """
    shapes = []
    ceiling = h * HORIZON
    for i in range(STARS):
        x = _rand(i, 41) * w
        # Squared, so they crowd the top of the frame and clear the treeline.
        y = (_rand(i, 42) ** 1.6) * ceiling * 0.94
        weight = _rand(i, 43) ** 2.4
        # Mostly slow. A handful are nearly steady, which is what gives the
        # restless ones something to be restless against.
        rate = 0.25 + 2.2 * _rand(i, 44) ** 2.0
        breath = 0.5 + 0.5 * math.sin(t * rate + _rand(i, 45) * TAU)
        # Hazier towards the horizon, as a real sky is.
        haze = 1.0 - 0.55 * (y / max(ceiling, 1.0))
        colour = STAR if _rand(i, 46) > 0.3 else STAR_COOL

        bloom = (1.7 + 3.0 * weight) * (0.9 + 0.2 * breath)
        shapes.append(
            _halo(
                _disc(x, y, bloom, bloom, 8),
                colour,
                colour,
                (16 + 104 * weight) * (0.5 + 0.5 * breath) * haze,
                0,
            )
        )
        # The point itself. Square, and never more than a couple of pixels: a
        # diamond this small loses its corners to the antialiasing and comes
        # back as the blur it was supposed to fix.
        core = 0.55 + 0.85 * weight
        shapes.append(
            _solid(
                [
                    _v(x - core, y - core),
                    _v(x + core, y - core),
                    _v(x + core, y + core),
                    _v(x - core, y + core),
                ],
                colour,
                alpha=(96 + 159 * weight) * (0.42 + 0.58 * breath) * haze,
            )
        )
        # Three or four of them carry arms. Any more and it is a Christmas
        # card, which is the whole risk of drawing a spike on a star at all.
        if weight > 0.90:
            shapes.append(
                _sparkle(x, y, bloom * 1.9, 84 * (0.3 + 0.7 * breath) * haze, colour)
            )
    return shapes


# (how many, the band of the frame they keep to, how far they wander, how big
# their light is, how sharp it is). Read as depth: the far swarm is many, tiny
# and pin-sharp, the near one is a handful, large and out of focus. They are
# drawn at three different points in the wood, so the same insect is sometimes
# behind a trunk and sometimes over it — and a firefly the size of the near
# ones, back among the deep trees, would be a lantern rather than a long way
# off.
SWARMS = (
    (170, -0.14, 0.06, 0.06, 0.026, 3.2, 1.00),
    (70, -0.09, 0.17, 0.08, 0.038, 5.6, 0.60),
    (14, 0.00, 0.28, 0.10, 0.050, 8.6, 0.20),
)


def _fireflies(w, h, t, swarm):
    """Points of light adrift in the wood, each on its own slow pulse.

    A firefly does not streak. It hangs, drifts a foot, hangs again — so the
    flight here is slow enough that a whole second of it would be a few pixels,
    and what is left to draw is the light itself: a soft round glow with a
    brighter point inside it, which is what one looks like across a clearing.

    The body is a *solid* disc with a glow around it, which is the whole of
    what makes it read as a light rather than a smudge. Two radial fades stacked
    on each other have no middle: they are falloff all the way down, and what
    arrives is a soft green stain. A flat bright disc with a soft ring around it
    is a lamp — the same thing that fixes a star, for the same reason.

    Depth is carried by *sharpness* as much as by size. The far ones are hard
    little points barely wider than a star; the near ones are wide, flat and
    dim, the way a light closer than anything the eye is focused on goes. Take
    the sharpness away and they all read as being at the same distance, however
    carefully their sizes are graded.
    """
    count, top, bottom, sway, rise, size, sharp = SWARMS[swarm]
    shapes = []
    for i in range(count):
        seed = swarm * 97 + i
        # Its own patch of the wood, and its own unhurried way around it.
        # Spaced across the width and then nudged, rather than scattered: a
        # dozen rolls of the dice leave half the wood empty and the other half
        # in a heap, and a swarm that clumps by accident looks like a bug.
        home_x = w * (-0.05 + 1.10 * (i + 0.5 + (_rand(seed, 61) - 0.5) * 1.3) / count)
        home_y = h * (HORIZON + top + (bottom - top) * _rand(seed, 62))
        speed = 0.05 + 0.08 * _rand(seed, 63)
        px, py = _rand(seed, 64) * TAU, _rand(seed, 65) * TAU
        # Not all the same lamp: a spread of sizes inside the swarm does more
        # for depth than the difference between one swarm and the next.
        own = size * (0.62 + 0.80 * _rand(seed, 68) ** 1.5)
        # Each on its own count and phase, so the wood blinks all over rather
        # than pulsing as one. With a swarm this size every one can fade the
        # whole way out and there are still plenty alight at any moment — which
        # is the wood the eye expects, dark with sparks coming and going in it.
        blink_rate = 0.9 + 2.1 * _rand(seed, 66)
        blink_phase = _rand(seed, 67) * TAU

        s = t * speed
        # What drift there is runs on two rates so it wanders rather than
        # orbits, but it hangs far more than it travels: a firefly is a light
        # held still and flashed, not one carried across the clearing.
        x = home_x + w * sway * (
            0.72 * math.sin(s * 1.7 + px) + 0.28 * math.sin(s * 4.3 + px * 2.1)
        )
        y = home_y + h * rise * math.sin(s * 2.3 + py)

        # A soft pulse that swells up and dies right back down to nothing, not a
        # lamp left half-on. A gentle exponent keeps each one visible through
        # most of its swell, so a whole field of them is always fading in and
        # out at once rather than the wood standing dark between flashes.
        glow = max(0.0, math.sin(t * blink_rate + blink_phase)) ** 1.3
        lit = 0.05 + 0.95 * glow

        # A radial fade is subdivided until it reads as smooth, which is dear —
        # and dearer still, every shape drawn costs a fixed toll no matter how
        # simple, so the way to keep a crowded wood cheap is fewer shapes, not
        # only cheaper ones. The background is drawn in flat discs for that: a
        # far firefly is a single warm dot, too small to want anything around
        # it; a mid one a bright dot over a faint wider one, a two-step glow that
        # never hollows the way a lone faint disc with a point in it does. Only
        # the near swarm, a handful of big soft ones up front, is worth a true
        # radial halo and the lamp inside it.
        if sharp > 0.85:
            r = own * (0.85 + 0.16 * _rand(seed, 69))
            shapes.append(
                _solid(_disc(x, y, r, r, 8), blend(FIREFLY, FIREFLY_CORE, 0.45), alpha=210 * lit)
            )
            continue
        if sharp > 0.4:
            r = own * (0.62 + 0.14 * _rand(seed, 69))
            shapes.append(_solid(_disc(x, y, r * 2.2, r * 2.2, 10), FIREFLY, alpha=68 * lit))
            shapes.append(
                _solid(_disc(x, y, r, r, 10), blend(FIREFLY, FIREFLY_CORE, 0.5), alpha=225 * lit)
            )
            continue
        shapes.append(
            _halo(
                _disc(x, y, own * 1.9, own * 1.9, 12),
                FIREFLY,
                FIREFLY,
                (120 + 60 * (1.0 - sharp)) * lit,
                0,
            )
        )
        # And the lamp itself: small and fierce far off, wide and mild close to.
        core = own * (0.62 - 0.40 * sharp)
        shapes.append(
            _solid(_disc(x, y, core, core, 10), FIREFLY_CORE, alpha=(118 + 137 * sharp) * lit)
        )
    return shapes


def _ground(w, h):
    """Everything below the ground line, darkening towards the near edge."""
    steps = 40
    top = [_v(w * i / steps, _ground_y(w, h, w * i / steps)) for i in range(steps + 1)]
    return _ramp(top + [_v(w, h), _v(0.0, h)], GROUND_NEAR, GROUND_FAR, 270.0, 255, 255)


def _mist(w, h, at, thickness, alpha):
    """A band of haze on the ground line, fading out of both its edges.

    Two halves meeting on the same line, one ramping up and one down. A single
    band can only fade one way, and the edge it does not fade is an edge — which
    makes a shape rather than weather. This is the whole of what puts air
    between one range of trees and the next.
    """
    shapes = []
    for side, angle, weight in ((-1.0, 270.0, alpha), (1.0, 90.0, alpha * 0.55)):
        steps = 24
        near, far = [], []
        for i in range(steps + 1):
            x = w * i / steps
            y = _ground_y(w, h, x) + at
            near.append(_v(x, y))
            far.append(_v(x, y + side * thickness))
        far.reverse()
        shapes.append(_ramp(near + far, MIST, MIST, angle, weight, 0))
    return shapes


def _range(w, h, spec, seed):
    """One range of trees with its haze, and the seed left where it got to.

    The range gives every tree its rough size and its colour; almost everything
    else about a particular tree is rolled here. A stand where only the height
    varies reads as one tree at several scales, which is what a range of firs
    must never look like — so the branch count, the width against the height,
    how far the branches fall and how much bare trunk shows are all rolled
    apart, and one tree in eight is an emergent that stands half again as tall
    as its neighbours.

    Sorted shortest-first within the range. They are all one flat shade, so
    overlapping silhouettes only read if the taller of any two wins — sort them
    the other way and a small tree in front of a big one simply disappears.
    """
    count, start, stop, drop, height, spread, width, tiers, droop, colour, haze = spec
    trees = []
    for i in range(count):
        seed += 1
        # Spaced along the range and then nudged hard enough to clump: an even
        # row of trees is a fence, and a wood is mostly gaps and thickets.
        u = start + (stop - start) * (i + 0.5 + (_rand(seed, 1) - 0.5) * 1.5) / count
        x = w * u
        tall = h * (height + spread * _rand(seed, 2)) * _canopy(u)
        # One in eight stands well clear of the rest. A canopy with nothing
        # coming through it is a hedge, however uneven its top edge is.
        if _rand(seed, 3) > 0.875:
            tall *= 1.42
        # Anything from a spindle to a broad fir, and anything from a young tree
        # branched to the ground to an old one with a long bare trunk.
        own_width = width * (0.66 + 0.68 * _rand(seed, 4))
        own_tiers = max(4, tiers + int(_rand(seed, 5) * 4.0) - 1)
        own_droop = droop * (0.72 + 0.62 * _rand(seed, 6))
        trunk = 0.07 + 0.26 * _rand(seed, 7) ** 1.6
        # And not all standing on the same line, so the range has some depth of
        # its own rather than being a row at one distance.
        base = _ground_y(w, h, x) + h * (drop + 0.016 * (_rand(seed, 8) - 0.35))
        trees.append(
            (
                tall,
                _solid(
                    _fir(
                        x,
                        base,
                        tall,
                        tall * own_width,
                        own_tiers,
                        seed,
                        trunk_frac=trunk,
                        droop=own_droop,
                    ),
                    colour,
                ),
            )
        )
    trees.sort(key=lambda pair: pair[0])
    shapes = [shape for _tall, shape in trees]
    shapes.extend(_mist(w, h, h * drop, h * 0.055, haze))
    return shapes, seed


class Forest:
    """The backdrop: a night wood, painted across the whole viewport.

    A background member is handed the viewport in the surface's own coordinates,
    so this paints across whatever it is given and never has to know where the
    plane over it has been dragged to.

    The trees do not move, so they are built once for a viewport size and kept.
    The stars and the fireflies do, so they are built every frame. That is the
    only reason the drawing is in four pieces rather than one: the cached
    stretches are the ones a frame has nothing new to say about, and the live
    ones are drawn between them so that a firefly can pass behind a trunk.
    """

    def __init__(self):
        self.started = time.monotonic()
        self._size = None
        # Sky, cloud and moon; then the wood, one range at a time.
        self._sky, self._deep, self._far, self._near = [], [], [], []

    def _build(self, w, h):
        self._sky = [_sky(w, h)] + _clouds(w, h) + _moon(w, h)

        seed = 0
        self._deep, seed = _range(w, h, RANGES[0], seed)
        self._far, seed = _range(w, h, RANGES[1], seed)
        self._near, seed = _range(w, h, RANGES[2], seed)
        self._near.append(_ground(w, h))

        for u, height in NEAR_TREES:
            seed += 1
            x = w * u
            tall = h * height
            self._near.append(
                _solid(
                    _fir(
                        x,
                        _ground_y(w, h, x) + h * 0.21,
                        tall,
                        tall * (0.26 + 0.14 * _rand(seed, 9)),
                        max(7, 9 + int(_rand(seed, 10) * 4.0) - 1),
                        seed,
                        trunk_frac=0.12 + 0.16 * _rand(seed, 11),
                        droop=1.15,
                    ),
                    TREE_NEAR,
                )
            )

    def draw(self, ctx):
        origin = ctx.constraints.pos
        avail = ctx.constraints.available()
        w, h = avail.x, avail.y
        t = time.monotonic() - self.started

        # The band's own box: everything is authored from (0, 0) and offset to
        # wherever the band was placed, at the last moment.
        box = dex.DrawConstraints(
            pos=origin,
            x=dex.AxisConstraint.Exactly(w),
            y=dex.AxisConstraint.Exactly(h),
            wrap=None,
            should_clip=True,
        )

        key = (round(w), round(h))
        if key != self._size:
            self._size = key
            self._build(w, h)

        # Sky, stars, and then the wood a range at a time with a swarm of
        # fireflies let loose between each pair of them.
        for shape in (
            self._sky
            + _stars(w, h, t)
            + self._deep
            + _fireflies(w, h, t, 0)
            + self._far
            + _fireflies(w, h, t, 1)
            + self._near
            + _fireflies(w, h, t, 2)
        ):
            ctx.draw_node(shape, box)

        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(origin, dex.Vector.new(w, h))
        )

    def type_name(self):
        return "A Forest"


# =======================================================================
# The fire
# =======================================================================
#
# `campfire.py`, with the hearth passed in rather than fixed, and with its own
# night backdrop dropped: the forest is behind it now, and the glow is supposed
# to land on the trees.

LOG_DARK = (36, 24, 23)
LOG_BODY = (50, 33, 27)
LOG_LIT = (98, 65, 42)
LOG_CAP = (104, 71, 47)

HEARTH_SHADOW = (5, 5, 10)

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

# How high the sparks get. In pixels rather than a fraction of the window: a
# fire is a fixed size, and a window twice as tall is more night above it and
# not a bigger fire.
SPARK_CLIMB = 250.0


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

    The width follows a bulge that comes to a point — the shape a flame actually
    takes. The sway is multiplied by `u` raised above one, so the foot stays
    planted on the log and only the tip whips about.
    """
    left, right = [], []
    for i in range(steps + 1):
        u = i / steps
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
    quite wrong in mid-air: it reads as a box.
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


def _hearth_shadow(hx, hy):
    """The dark the logs sit in, which is what gives them any weight.

    Soft-edged and round: a flat ellipse under a lit clearing reads as a hole
    cut in it, and a radial ramp reaches nothing exactly where a circle ends.
    """
    return _halo(
        _disc(hx, hy + 30.0, 106.0, 106.0, 14),
        HEARTH_SHADOW,
        HEARTH_SHADOW,
        near_alpha=170,
        far_alpha=0,
    )


def _logs(hx, hy):
    """Three logs: one across the back, two crossed in front of it.

    Each is four flat shapes — a body, a narrow lit face along its upper edge, a
    shadow along its lower one, and an end cap. Firelight comes from one place,
    so a thin strip of a round log is lit and the rest of it is turned away.
    """
    shapes = []
    for cx, cy, half_len, radius, angle in (
        (hx + 4.0, hy + 20.0, 96.0, 15.0, 0.06),
        (hx - 26.0, hy + 34.0, 84.0, 13.0, -0.30),
        (hx + 30.0, hy + 36.0, 80.0, 12.0, 0.26),
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


def _embers(hx, hy, t):
    """The bed the flames stand in, each coal breathing on its own count."""
    shapes = []
    for i in range(7):
        heat = flicker(t, phase=i * 1.7)
        x = hx + (_rand(i, 11) - 0.5) * 132.0
        y = hy + 10.0 + (_rand(i, 12) - 0.5) * 22.0
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


def _wash(hx, hy, t):
    """The light the fire throws into the wood, pulsing with it.

    Round, and drawn with nothing opaque under it, so it lands on the trees. A
    radial ramp runs from the middle of a shape to its furthest corner, so on
    anything but a circle it is still lit where the outline is — and that shows
    as the edge of an ellipse hanging in the dark.
    """
    beat = flicker(t, phase=0.4)
    radius = 210.0 + 28.0 * beat
    return _halo(
        _disc(hx, hy - 16.0, radius, radius, 16),
        GLOW,
        GLOW,
        near_alpha=int(96 + 38 * beat),
        far_alpha=0,
    )


def _hearth_light(hx, hy, t):
    """The near light, drawn over the logs so that it lands on them.

    Without this the wood is lit from nowhere: the fire is standing on it and
    the two never meet.
    """
    beat = flicker(t, phase=2.2)
    radius = 78.0 + 12.0 * beat
    return _halo(
        _disc(hx, hy + 4.0, radius, radius, 28),
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


def _flames(hx, hy, t):
    shapes = []
    breath = 0.86 + 0.24 * flicker(t, phase=1.1)
    for dx, half, height, lean, wobble, speed, phase, near, far, alpha in FLAMES:
        # Each tongue breathes on its own count as well as with the fire, so
        # they never all reach for the top at once.
        own = 0.80 + 0.34 * flicker(t, phase=phase * 1.6)
        shapes.append(
            _ramp(
                tongue(
                    hx + dx,
                    hy + 2.0,
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
                    hx + dx,
                    hy + 10.0,
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


def _wisps(hx, hy, t):
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
                    hx + dx + math.sin(u * 4.0 + i) * 13.0,
                    hy - 96.0 - rise,
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


def _sparks(hx, hy, t):
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
        rise = SPARK_CLIMB * (0.40 + 0.60 * _rand(i, 3))
        # Fast off the fire, slowing as they go.
        climb = 1.0 - (1.0 - u) ** 1.8
        x = hx + (_rand(i, 4) - 0.5) * 46.0
        x += math.sin(climb * 4.6 + _rand(i, 5) * TAU) * (14.0 + 30.0 * _rand(i, 6)) * climb
        y = hy - 10.0 - rise * climb
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


# How far in front of the ground line the camp is made, and the closest to the
# foot of the window it is allowed to get.
FIRE_DROP = 0.17
FIRE_MARGIN = 84.0


class Campfire:
    """A fire, drawn from the back of the scene to the front.

    Nothing in dex has to be told to animate. The application asks for the next
    frame the moment it finishes one, so this `draw` runs continuously, and a
    drawing that reads the clock is a drawing that moves. There is no frame
    counter to subscribe to and no timer to install — `time.monotonic()` is the
    whole of it.

    A foreground member, so it is pinned to the viewport exactly as the forest
    behind it is, and the camp holds together however the window is dragged
    about. It draws no sensors, so it takes nothing from the plane underneath:
    the surface still pans when you drag the night, and the items on it are
    still there to be picked up.
    """

    def __init__(self):
        # When this node was made, so the fire starts where it is placed rather
        # than somewhere in the middle of the process's lifetime.
        self.started = time.monotonic()

    def draw(self, ctx):
        origin = ctx.constraints.pos
        avail = ctx.constraints.available()
        w, h = avail.x, avail.y
        t = time.monotonic() - self.started

        box = dex.DrawConstraints(
            pos=origin,
            x=dex.AxisConstraint.Exactly(w),
            y=dex.AxisConstraint.Exactly(h),
            wrap=None,
            should_clip=True,
        )

        # The camp is made in the clearing: centred, and a little way in front
        # of the treeline the backdrop drew — the same ground line, so the two
        # bands agree about where the ground is without either being told.
        hx = w * 0.5
        hy = min(_ground_y(w, h, hx) + h * FIRE_DROP, h - FIRE_MARGIN)

        # Back to front: the light thrown into the trees, the dark under the
        # logs, the wood, the light landing *on* the wood, then the fire.
        for shape in (
            [_wash(hx, hy, t), _hearth_shadow(hx, hy)]
            + _logs(hx, hy)
            + [_hearth_light(hx, hy, t)]
            + _embers(hx, hy, t)
            + _flames(hx, hy, t)
            + _wisps(hx, hy, t)
            + _sparks(hx, hy, t)
        ):
            ctx.draw_node(shape, box)

        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(origin, dex.Vector.new(w, h))
        )

    def type_name(self):
        return "A Campfire"


# =======================================================================
# The surface
# =======================================================================


def build(ws):
    """A canvas with the forest behind it and the fire in front; returns its uid.

    Every id is minted here rather than read back: the action queue does not
    drain until this returns, so nothing built along the way can be looked up
    before then.
    """
    canvas = dex.Canvas.build(ws)
    # Named, so the lens, the breadcrumb trail and the tab all call it what it
    # is rather than "A Canvas".
    ws.submit_action(canvas, dex.NameCanvas("Camp"), "Named the surface")

    forest = dex.NodeUid.mint()
    ws.insert_node_at_dyn(forest, Forest())
    ws.submit_action(
        canvas, dex.AdoptCanvasNode(forest, dex.Layer.background()), "Planted the wood"
    )

    fire = dex.NodeUid.mint()
    ws.insert_node_at_dyn(fire, Campfire())
    ws.submit_action(
        canvas, dex.AdoptCanvasNode(fire, dex.Layer.foreground()), "Lit the fire"
    )

    # And two things on the plane between them, pale enough to be read against
    # the trees. Drag either one, or drag the night itself: the camp does not
    # move, because neither band is on the plane.
    for i, (text, x, y) in enumerate(
        (("drag me", 150.0, 150.0), ("the camp stays put", 330.0, 230.0))
    ):
        label = dex.LabelEditable.new(text)
        label.color = dex.Color.rgba(196, 206, 228, 255)
        child = dex.NodeUid.mint()
        ws.insert_node_at_dyn(child, label)
        item = dex.CanvasNode.build(ws, child, dex.Vector.new(x, y), dex.Vector.new(190.0, 30.0))
        ws.submit_action(
            canvas, dex.AdoptCanvasNode(item, dex.Layer.midground()), f"Placed {i}"
        )

    return canvas


def transform():
    """A lambda returning a canvas you can take to a desktop of its own."""
    return build(dex.ws)
