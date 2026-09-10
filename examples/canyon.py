"""Looking down a meandering red-rock canyon, modelled as faceted strata walls.

An *image-style* node, the same kind as `tshirt.py`: authored in world units and
projected into whatever box it is handed — uniform scale, centred, letterboxed.
All geometry is plain numbers turned into `dex.Path`s at draw time, so the node
stays picklable.

It is not a picture of a canyon; it is a little 3-D corridor seen through a
pinhole camera set low on the river. The canyon is a centre-line that meanders in
`x` as it recedes in `z`, with a wall rising on each side. Each wall is a stack
of strata *benches* — flat layers that step outward as they climb — and every
bench, over every depth segment, is one flat-shaded facet lit by a single sun.
So the strata are real horizontal bands, the low-poly faceting is erosion
displacing each node in and out, and the perspective (near walls tall and spilling
past the frame, everything converging to one horizon) is a real projection rather
than a drawn guess.

Because the centre-line meanders, the river swings across the floor and slides
behind a wall instead of funnelling to a point, and the far rock washes into haze
— so the gorge reads as bending away and continuing, not ending. Painter's order
(far segments first) does the occlusion, so a near bluff hides the reach behind
it exactly as it should.
"""

import math

TAU = math.tau

ART_W = 1000.0
ART_H = 640.0

# --- palette ----------------------------------------------------------------
# The sky is the only gradient. The rock is flat facets, stepping through strata
# from a deep oxblood at the water to a bleached rim; the river is slate that
# glints where a facet turns to the sun; the far distance washes into HAZE.
SKY_TOP = (60, 118, 188)
SKY_LOW = (182, 214, 236)
HAZE = (192, 188, 196)
SUN = (255, 246, 216)

# Strata, water-level up to rim — one colour per bench, so the layering reads as
# crisp horizontal bands.
STRATA = [
    (110, 52, 48),
    (130, 60, 50),
    (138, 60, 46),
    (158, 74, 52),
    (178, 92, 60),
    (192, 108, 70),
    (202, 126, 84),
    (212, 146, 102),
    (218, 164, 122),
    (224, 182, 142),
    (230, 198, 158),
]

SAND = (196, 150, 108)
WATER = (48, 100, 142)
WATER_HI = (156, 200, 218)

# The sun, world space (x right, y up, z into the scene — so -z is toward the
# camera). Light spilling in from the upper left and a little toward us.
LIGHT = (-0.50, 0.62, -0.52)
AMBIENT = 0.70
DIFFUSE = 0.62


def _v(x, y):
    return dex.Vector.new(float(x), float(y))


def rgb(c, alpha=255):
    return dex.Color.rgba(int(c[0]), int(c[1]), int(c[2]), int(alpha))


def blend(a, b, t):
    t = min(1.0, max(0.0, t))
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def scale(c, f):
    return tuple(min(255.0, max(0.0, ci * f)) for ci in c)


def clamp(x, lo=0.0, hi=1.0):
    return lo if x < lo else hi if x > hi else x


def fbm(x, z):
    """Cheap smooth deterministic noise: three sines that never realign."""
    return (1.00 * math.sin(x * 0.0110 + z * 0.0130)
            + 0.50 * math.sin(x * 0.0250 - z * 0.0190 + 2.1)
            + 0.25 * math.sin(x * 0.0430 + z * 0.0370 + 4.0))


# =======================================================================
# Path builders
# =======================================================================


def solid(tf, pts, colour, alpha=255):
    return dex.Path.polygon([tf(x, y) for (x, y) in pts], rgb(colour, alpha),
                            dex.Stroke.none())


def ramp(tf, pts, near, far, angle, near_alpha=255, far_alpha=255):
    path = dex.Path.polygon([tf(x, y) for (x, y) in pts], rgb(near, near_alpha),
                            dex.Stroke.none())
    path.fill_mode = dex.FillMode.linear()
    path.fill_end = rgb(far, far_alpha)
    path.fill_angle = angle
    return path


def disc(tf, cx, cy, r, colour, alpha=255, sides=30):
    pts = [(cx + r * math.cos(TAU * i / sides), cy + r * math.sin(TAU * i / sides))
           for i in range(sides)]
    return solid(tf, pts, colour, alpha)


# =======================================================================
# The corridor — a meandering canyon in world space
# =======================================================================

NZ = 38            # depth segments
BENCHES = 11       # strata beds per wall
RIM = 1300.0       # wall height, water to rim — the walls run off the top of
                   # the frame for all but the farthest reach, which is the
                   # whole point: you cannot see out of this thing.
BENCH_STEP = 24.0  # the unit each bed's setback is measured in
EROD_X = 70.0      # in/out erosion of the wall nodes (the faceting)
FLUTE = 34.0       # depth of the vertical runoff fluting cut into the cliff
EROD_Y = 26.0      # waviness of a bedding plane along the canyon

# A real section is not a stack of equal slabs. These are the bed thicknesses,
# water to rim, and how far each bed's top has retreated from the one below —
# the two together are what makes a cliff band alternate with a ledge, and what
# gives the wall its profile instead of a uniform ramp.
BED_H = [1.30, 0.50, 1.55, 0.40, 0.95, 1.45, 0.55, 1.15, 0.45, 1.35, 0.60]
BED_SET = [0.30, 1.70, 0.25, 1.90, 0.85, 0.30, 1.55, 0.45, 1.65, 0.35, 1.20]

_H_CUM = [0.0]
for _w in BED_H:
    _H_CUM.append(_H_CUM[-1] + _w)
BED_Y = [RIM * c / _H_CUM[-1] for c in _H_CUM]

_S_CUM = [0.0]
for _w in BED_SET:
    _S_CUM.append(_S_CUM[-1] + _w)
BED_X = [BENCH_STEP * c for c in _S_CUM]

Z_NEAR = 58.0
Z_FAR = 4200.0
WATER_HALF = 100.0  # half-width of the river ribbon on the floor

# The pinhole camera. Not down on the water — a third of the way up the wall,
# on a wide lens, tipped down at the river. From the floor you can see a hundred
# yards and a wall; from here the reach opens out and runs away in front of you,
# with eight hundred feet of rock still overhead.
CAM_Y = 430.0
PITCH = 0.185
FOCAL = 520.0
CX, CY = 500.0, 322.0

_CP, _SP = math.cos(PITCH), math.sin(PITCH)


def centre(z):
    """The canyon's centre-line in x at depth z — the meander.

    Zeroed at the camera, which is what puts us in the middle of the channel
    looking straight down it rather than pressed against a wall. Three swings at
    three wavelengths, and all of them held under the channel's own half-width:
    that is the difference between a gorge that keeps going and one that ends.
    Each swing noses a spur in from one side without ever quite crossing the
    view, so the reach reads as a run of interlocking headlands, each paler than
    the one in front of it, with more canyon showing in the gap past every one.
    Only at the very end of the reach, deep in the haze, do the walls finally
    close across each other."""
    return (83.0 * math.sin(z * 0.00470 + 0.20)
            + 60.0 * math.sin(z * 0.00178 + 2.20)
            + 53.0 * math.sin(z * 0.00060) - 64.98)


def half_width(z):
    """Half the floor width at depth z. The gorge pinches and opens as it goes,
    and over the whole reach it widens: side canyons keep joining downstream, so
    the far distance is a broader gorge behind the near one rather than a slot
    tapering to nothing."""
    return (200.0 + 52.0 * math.sin(z * 0.00170 + 0.6)) * (1.0 + z / 4600.0)


def project(x, y, z):
    """World -> (screen x, screen y, camera depth). Pinhole with a down-pitch."""
    yc = y - CAM_Y
    y2 = yc * _CP + z * _SP
    z2 = -yc * _SP + z * _CP
    if z2 < 1.0:
        z2 = 1.0
    return (CX + FOCAL * x / z2, CY - FOCAL * y2 / z2, z2)


def z_at(j):
    """Depth of segment index `j`, spaced so screen steps stay even."""
    return Z_NEAR * (Z_FAR / Z_NEAR) ** (j / NZ)


def _norm(a):
    L = math.sqrt(a[0] * a[0] + a[1] * a[1] + a[2] * a[2]) or 1.0
    return (a[0] / L, a[1] / L, a[2] / L)


_LIGHT = _norm(LIGHT)


def rough(z, side):
    """How chewed-up this stretch of wall is. It varies slowly along the gorge,
    so a smooth sheer face gives way to a deeply fluted one and back."""
    return 0.60 + 0.80 * (fbm(z * 0.13 + side * 210.0, 3.0) / 1.75 * 0.5 + 0.5)


def buttress(z, side):
    """The slow in/out swing of a whole wall column. Positive pushes the column
    into the gorge (a promontory), negative scoops it back (an alcove) — and the
    scoop is the deeper of the two, because that is how side-canyons cut."""
    f = fbm(z * 0.42 + side * 900.0, 17.0) / 1.75
    return 42.0 * f if f > 0.0 else 86.0 * f


def wall_node(j, b, side):
    """A world point on a wall: depth segment `j`, bench `b`, `side` -1 (left)
    or +1 (right). Each layer steps outward as it climbs, and erosion pushes the
    node in and out so the facets it belongs to tilt every which way. Three
    slower terms give the wall its shape: `buttress` swings the whole column,
    `step` varies how far the terraces set back, and `hf` lifts and drops the
    rim so the skyline breaks into mesas and saddles instead of running level."""
    z = z_at(j)
    r = rough(z, side)
    hf = 1.0 + 0.26 * fbm(z * 0.55 + side * 310.0, 71.0) / 1.75
    y = BED_Y[b] * hf + EROD_Y * fbm(z * 0.7, b * 130.0 + side * 40.0)
    setback = BED_X[b] * (1.0 + 0.45 * fbm(z * 0.30 + side * 77.0, 5.0) / 1.75)
    base = centre(z) + side * (half_width(z) + setback - buttress(z, side))
    ex = EROD_X * r * fbm(z * 1.3 + side * 500.0, b * 220.0 + 60.0)
    flute = FLUTE * r * (0.60 * math.sin(j * 2.3 + side * 1.7)
                         + 0.40 * math.sin(j * 0.9 + side * 4.4))
    flute *= 0.35 + 0.65 * b / BENCHES
    return (base + side * abs(ex) * 0.6 + ex * 0.4 - side * flute, y, z)


def floor_node(j, u):
    """A world point on the floor: `u` in [-1, 1] across the channel."""
    z = z_at(j)
    x = centre(z) + u * WATER_HALF
    ripple = 1.5 * fbm(x * 4.5, z * 5.0 + 5.0)
    return (x, ripple, z)


def water_colour(z):
    """The river's colour at depth `z`. Water is a mirror, not a surface: its
    colour comes from the sky it reflects and from where the sun sits on it, and
    both change smoothly along the reach — so the river is shaded by depth alone
    and never breaks into facets."""
    zt = clamp((z - Z_NEAR) / (Z_FAR - Z_NEAR))
    col = blend(WATER, WATER_HI, clamp(zt * 1.5) ** 0.8)
    glint = clamp(0.5 + 0.5 * math.sin(z * 0.0042 + 0.7))
    col = blend(col, WATER_HI, 0.20 * glint)
    return blend(col, HAZE, clamp((zt ** 1.6) * 0.72, 0.0, 0.42))


def facet(pts_world, base, zt):
    """One flat facet of rock: shade by its normal against the sun, tint by its
    bed, wash toward haze with distance."""
    p0, p1, p2 = pts_world[0], pts_world[1], pts_world[2]
    ux = (p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2])
    vx = (p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2])
    n = _norm((ux[1] * vx[2] - ux[2] * vx[1],
               ux[2] * vx[0] - ux[0] * vx[2],
               ux[0] * vx[1] - ux[1] * vx[0]))
    # Face the normal toward the camera, so the side we see is the side we light.
    cx = sum(p[0] for p in pts_world) / len(pts_world)
    cy = sum(p[1] for p in pts_world) / len(pts_world)
    cz = sum(p[2] for p in pts_world) / len(pts_world)
    to_cam = _norm((0.0 - cx, CAM_Y - cy, 0.0 - cz))
    if n[0] * to_cam[0] + n[1] * to_cam[1] + n[2] * to_cam[2] < 0.0:
        n = (-n[0], -n[1], -n[2])
    d = max(0.0, n[0] * _LIGHT[0] + n[1] * _LIGHT[1] + n[2] * _LIGHT[2])
    up = max(0.0, n[1])
    shade = min(1.12, AMBIENT * (0.72 + 0.40 * up) + DIFFUSE * d)
    col = scale(base, shade)
    # Skylight bounce: the side the sun misses is lit by the sky, so it goes
    # blue-grey rather than muddy — most of all where a facet looks upward.
    col = blend(col, SKY_LOW, 0.20 * (1.0 - d) * (0.35 + 0.65 * up))
    # A little tonal mottling, so a broad sunlit bank is not one flat wash.
    col = scale(col, 1.0 + 0.055 * fbm(cx * 1.7, cz * 1.9 + 11.0))
    return blend(col, HAZE, clamp((zt ** 1.6) * 0.72, 0.0, 0.42))


def add_facet(faces, nodes, base):
    """Project one flat facet, shade it, and file it under its camera depth.
    The fourth slot is the far end of a gradient, which rock never uses."""
    screen = [project(*w) for w in nodes]
    zt = clamp((sum(w[2] for w in nodes) / len(nodes) - Z_NEAR) / (Z_FAR - Z_NEAR))
    colour = facet(nodes, base, zt)
    depth = sum(p[2] for p in screen) / len(screen)
    faces.append((depth, [(p[0], p[1]) for p in screen], colour, None))


def add_water(faces, nodes, z_near, z_far):
    """One length of river: a single quad across the whole channel, filled with
    a gradient between its two ends. One quad and no normals, so the water comes
    out as a smooth ribbon rather than a run of glinting triangles."""
    screen = [project(*w) for w in nodes]
    depth = sum(p[2] for p in screen) / len(screen)
    faces.append((depth, [(p[0], p[1]) for p in screen],
                  water_colour(z_near), water_colour(z_far)))


def aerial(colour, z):
    """Wash a colour into the haze by depth — the same aerial perspective the
    rock gets, so anything standing in this world stands in the same air."""
    zt = clamp((z - Z_NEAR) / (Z_FAR - Z_NEAR))
    return blend(colour, HAZE, clamp((zt ** 1.6) * 0.72, 0.0, 0.42))


def beam(faces, p0, p1, w, colour):
    """A world segment drawn as a screen-space quad whose width thins with
    distance — a wing, a timber, a wire."""
    (x0, y0, d0) = project(*p0)
    (x1, y1, d1) = project(*p1)
    length = math.hypot(x1 - x0, y1 - y0) or 1.0
    (nx, ny) = (-(y1 - y0) / length, (x1 - x0) / length)
    h0 = w * FOCAL / d0 * 0.5
    h1 = w * FOCAL / d1 * 0.5
    pts = [(x0 + nx * h0, y0 + ny * h0), (x1 + nx * h1, y1 + ny * h1),
           (x1 - nx * h1, y1 - ny * h1), (x0 - nx * h0, y0 - ny * h0)]
    faces.append(((d0 + d1) * 0.5, pts,
                  aerial(colour, (p0[2] + p1[2]) * 0.5), None))


def panel(faces, pts_world, colour):
    """A flat panel painted in a colour of its own, rather than shaded off the
    sun — a shadow on the sand, glass with the afternoon in it, a lens."""
    screen = [project(*p) for p in pts_world]
    z = sum(p[2] for p in pts_world) / len(pts_world)
    faces.append((sum(s[2] for s in screen) / len(screen),
                  [(s[0], s[1]) for s in screen], aerial(colour, z), None))


# Ravens, strung down the gorge — (across the channel, height over the water,
# depth, wingspan). Nothing states the size of a wall like something small in
# front of it, and a 12-unit bird against a 1300-unit cliff states it flatly.
BIRD = (54, 48, 54)
BIRDS = [(-0.34, 402.0, 660.0, 15.0),
         (0.22, 486.0, 800.0, 14.0),
         (-0.06, 356.0, 950.0, 13.0),
         (0.46, 540.0, 1180.0, 15.0),
         (-0.30, 448.0, 1440.0, 14.0),
         (0.16, 610.0, 1760.0, 15.0)]


def build_birds(faces):
    """Each raven is two strokes of a shallow V, held open on the thermal."""
    for (u, y, z, span) in BIRDS:
        x = centre(z) + u * half_width(z)
        (tip, rise) = (span * 0.5, span * 0.34)
        droop = span * 0.10
        beam(faces, (x - tip, y + rise, z), (x, y - droop, z), span * 0.15, BIRD)
        beam(faces, (x, y - droop, z), (x + tip, y + rise, z), span * 0.15, BIRD)


def build_corridor(faces):
    """Every facet of both walls and the floor."""

    def add(nodes, base):
        add_facet(faces, nodes, base)

    for j in range(NZ):
        # Floor: sand on each side of a water ribbon.
        fl0, fl1 = floor_node(j, -1.0), floor_node(j + 1, -1.0)
        fr0, fr1 = floor_node(j, 1.0), floor_node(j + 1, 1.0)
        wlL0 = wall_node(j, 0, -1)
        wlL1 = wall_node(j + 1, 0, -1)
        wlR0 = wall_node(j, 0, 1)
        wlR1 = wall_node(j + 1, 0, 1)
        add([wlL0, fl0, fl1, wlL1], SAND)
        add([fr0, wlR0, wlR1, fr1], SAND)
        # The river itself: one quad the full width of the channel.
        add_water(faces, [fl0, fr0, fr1, fl1], z_at(j), z_at(j + 1))

        # Walls: one facet per bench per side, split on the diagonal so each
        # half is its own flat facet — the low-poly look.
        for side in (-1, 1):
            for b in range(BENCHES):
                a = wall_node(j, b, side)
                bb = wall_node(j, b + 1, side)
                c = wall_node(j + 1, b + 1, side)
                dd = wall_node(j + 1, b, side)
                base = STRATA[min(b, len(STRATA) - 1)]
                add([a, bb, c], base)
                add([a, c, dd], base)


def paint(tf, faces):
    """Painter's algorithm: the farthest facet first, so a near bluff hides the
    reach behind it without anyone having to keep a depth buffer."""
    faces.sort(key=lambda it: -it[0])
    return [solid(tf, pts, col) if far is None else ramp(tf, pts, col, far, 270.0)
            for (_d, pts, col, far) in faces]


def build_scene(tf):
    shapes = []
    shapes.append(ramp(tf, [(0, 0), (ART_W, 0), (ART_W, ART_H), (0, ART_H)],
                       SKY_LOW, SKY_TOP, 270.0))
    shapes.append(disc(tf, 800, 96, 82, SUN, alpha=66, sides=32))
    shapes.append(disc(tf, 800, 96, 38, SUN, sides=28))
    faces = []
    build_corridor(faces)
    build_birds(faces)
    shapes += paint(tf, faces)
    return shapes


# =======================================================================
# The image node (fit-to-box, from tshirt.py)
# =======================================================================


class Canyon:
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
            return _v(x * s, y * s)
        tf.s = s

        box = dex.DrawConstraints(
            pos=dex.ScreenPos.new(origin.x + dx, origin.y + dy),
            x=dex.AxisConstraint.Exactly(self.art_w * s),
            y=dex.AxisConstraint.Exactly(self.art_h * s),
            wrap=None, should_clip=True)
        for shape in build_scene(tf):
            ctx.draw_node(shape, box)
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(origin, dex.Vector.new(w, h)))

    def type_name(self):
        return "A Canyon"


def transform():
    """A meandering red-rock canyon, modelled as faceted strata walls."""
    return Canyon()
