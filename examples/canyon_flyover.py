"""`canyon_watchtower.py` with a small plane climbing out of the gorge.

Same canyon, same lookout on the bank, and above them an aircraft banking up out
of the reach into the open sky, its contrail still hanging in the air behind it
all the way back down to the bend it came round.

The plane is authored once in its own frame, nose along +x, as a planform
silhouette — fuselage, two swept wings, two tailplanes — which is the view you
get of an aircraft seen from below and behind as it climbs away from you. Where
it hangs, how big it comes out and which way it is pointed are not drawn in:
they are read off two world points, the plane and the mouth it climbed out of.
The projection sizes it, the screen-space bearing between those two points turns
it, and the contrail is one tapering quad run between them, so it narrows into
the distance and dissolves into the same haze the far rock does.
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

# The timber, its glass and its brass. These are base colours: the tower is lit
# by the same sun as the rock, so what is drawn is these shaded, not these.
WOOD_LIT = (170, 120, 72)
WOOD = (126, 84, 50)
WOOD_DARK = (82, 52, 32)
ROOF = (104, 64, 46)
GLASS = (96, 122, 138)
GLASS_LIT = (250, 224, 150)
METAL = (74, 80, 92)
METAL_LIT = (150, 158, 172)
LENS = (206, 232, 244)

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

Z_NEAR = 34.0
Z_FAR = 4200.0
WATER_HALF = 100.0  # half-width of the river ribbon on the floor

# The pinhole camera. Not down on the water — a third of the way up the wall,
# on a wide lens, tipped down at the river. From the floor you can see a hundred
# yards and a wall; from here the reach opens out and runs away in front of you,
# with eight hundred feet of rock still overhead.
CAM_Y = 430.0
PITCH = 0.185
FOCAL = 200.0    # much wider than `canyon.py`: same camera in the same
                 # place, tipped the same way, but standing well back — there is
                 # a mast in here that clears the rim, and all of it has to fit
CX, CY = 500.0, 386.0

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


# =======================================================================
# The firewatch tower
# =======================================================================
#
# Built in world space and pushed through the camera above. The legs are straight
# lines from a wide footing to a narrow deck, so the position of a leg at any
# height is a lerp, and the braces, ties and railing hang off those lerps rather
# than off numbers guessed to line up.

TOWER_Z = 1000.0   # how far down the gorge the tower stands
TOWER_SIDE = 1     # the sunlit bank
TOWER_T = 0.62     # across the bank: 0 at the water, 1 at the foot of the cliff
T_H = 1360.0       # footing to deck. `RIM` is 1300, so the cabin sits in open
                   # sky above the canyon's own top — which is the whole idea of
                   # a lookout, and makes this a lattice mast rather than a
                   # cabin on stilts: nearly ten times as tall as it is wide
T_WB = 66.0        # half-footprint at the ground
T_WT = 24.0        # half-footprint at the deck
T_LEVELS = 15      # bracing bays up the legs
LEG_W = 24.0       # timber widths, in world units
BRACE_W = 14.0
RAIL_W = 10.0

CORNERS = ((-1, -1), (1, -1), (1, 1), (-1, 1))


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0])


def lit(colour, outward):
    """Shade a timber by the way its outward face is turned — the same sun, the
    same skylight fill, as every facet of the canyon."""
    n = _norm(outward)
    d = max(0.0, n[0] * _LIGHT[0] + n[1] * _LIGHT[1] + n[2] * _LIGHT[2])
    return scale(colour, AMBIENT * 0.88 + DIFFUSE * d)


def tower_base():
    """Where the legs meet the sand: a point across the bank at `TOWER_Z`, read
    off the same centre-line and channel width the floor itself is built from."""
    z = TOWER_Z
    s = TOWER_SIDE
    inner = centre(z) + s * WATER_HALF
    outer = centre(z) + s * (half_width(z) - buttress(z, s))
    return (inner + (outer - inner) * TOWER_T, 0.0, z)


BASE = tower_base()


def leg_point(c, u):
    """Corner `c` of the tower at height fraction `u`."""
    (sx, sz) = CORNERS[c]
    half = T_WB + (T_WT - T_WB) * u
    return (BASE[0] + sx * half, BASE[1] + T_H * u, BASE[2] + sz * half)


def face_out(i, j):
    """The outward normal of the tower side running from corner `i` to `j`."""
    (ax, az) = CORNERS[i]
    (bx, bz) = CORNERS[j]
    return _norm(((ax + bx) * 0.5, 0.0, (az + bz) * 0.5))


def square(cx, cz, half, y):
    """The four corners of a square of half-width `half` at height `y`."""
    return [(cx + sx * half, y, cz + sz * half) for (sx, sz) in CORNERS]


def build_tower(faces):
    (bx, _by, bz) = BASE

    # The shadow the tower drops on the sand, thrown away from the sun. It is
    # flat on the floor, so the projection turns the circle into the ellipse it
    # ought to be without anyone drawing one.
    shadow = [(bx + 1.5 * T_WB * math.cos(TAU * k / 20) + 128.0, 1.5,
               bz + 1.0 * T_WB * math.sin(TAU * k / 20) + 44.0) for k in range(20)]
    panel(faces, shadow, scale(SAND, 0.60))

    # The truss: four sides of X-bracing between the legs, tied off at each bay.
    for (a, b) in ((0, 1), (1, 2), (2, 3), (3, 0)):
        out = face_out(a, b)
        brace = lit(WOOD_DARK, out)
        tie = lit(WOOD, out)
        for lvl in range(T_LEVELS):
            u0 = lvl / T_LEVELS
            u1 = (lvl + 1) / T_LEVELS
            (a0, a1) = (leg_point(a, u0), leg_point(a, u1))
            (b0, b1) = (leg_point(b, u0), leg_point(b, u1))
            beam(faces, a0, b1, BRACE_W, brace)
            beam(faces, b0, a1, BRACE_W, brace)
            beam(faces, a1, b1, BRACE_W * 1.25, tie)

    # The legs, over their own bracing.
    for c in range(4):
        (sx, sz) = CORNERS[c]
        beam(faces, leg_point(c, 0.0), leg_point(c, 1.0), LEG_W,
             lit(WOOD, (sx, 0.0, sz)))

    # The deck: a slab a little wider than the legs. The camera stands well below
    # it, so what reads is the underside and the skirt, not the floor.
    deck = square(bx, bz, T_WT + 30.0, T_H)
    under = [(p[0], T_H - 26.0, p[2]) for p in deck]
    add_facet(faces, under, WOOD_DARK)
    for i in range(4):
        j = (i + 1) % 4
        add_facet(faces, [deck[i], deck[j], under[j], under[i]], WOOD_LIT)

    # The railing: posts around the deck, and a rail along their tops.
    rail_h = 46.0
    for i in range(4):
        j = (i + 1) % 4
        out = face_out(i, j)
        (p, q) = (deck[i], deck[j])
        for k in range(5):
            t = k / 4.0
            (px, pz) = (p[0] + (q[0] - p[0]) * t, p[2] + (q[2] - p[2]) * t)
            beam(faces, (px, T_H, pz), (px, T_H + rail_h, pz), RAIL_W,
                 lit(WOOD_DARK, out))
        beam(faces, (p[0], T_H + rail_h, p[2]), (q[0], T_H + rail_h, q[2]),
             RAIL_W * 1.4, lit(WOOD, out))

    # The cabin: a box on the deck, under a flat roof with a wide eave.
    cw = T_WT + 22.0
    (cy0, cy1) = (T_H, T_H + 118.0)
    low = square(bx, bz, cw, cy0)
    high = square(bx, bz, cw, cy1)
    for i in range(4):
        j = (i + 1) % 4
        add_facet(faces, [low[i], low[j], high[j], high[i]], WOOD)

    # The glass, glazed all round the way a lookout is and set just proud of each
    # wall so it wins the depth sort against the wall it is set into. The panes
    # the sun is behind catch the afternoon; the rest stay slate.
    for i in range(4):
        j = (i + 1) % 4
        out = face_out(i, j)
        (p, q) = (low[i], low[j])
        pane = [(p[0] + (q[0] - p[0]) * t + out[0] * 1.2, y,
                 p[2] + (q[2] - p[2]) * t + out[2] * 1.2)
                for (t, y) in ((0.15, cy0 + 26.0), (0.85, cy0 + 26.0),
                               (0.85, cy1 - 24.0), (0.15, cy1 - 24.0))]
        facing = out[0] * _LIGHT[0] + out[2] * _LIGHT[2]
        panel(faces, pane, blend(GLASS, GLASS_LIT, clamp(facing * 1.4)))

    # The roof: a slab with an eave all round.
    eave = square(bx, bz, cw + 18.0, cy1)
    cap = square(bx, bz, cw + 18.0, cy1 + 22.0)
    add_facet(faces, cap, ROOF)
    for i in range(4):
        j = (i + 1) % 4
        add_facet(faces, [eave[i], eave[j], cap[j], cap[i]], ROOF)

    build_telescope(faces, bx - T_WT - 26.0, T_H + 6.0, bz - T_WT - 26.0)


def build_telescope(faces, mx, my, mz):
    """A telescope on a tripod at the deck rail, tube cocked up and out over the
    gorge — the reason anyone climbs the ladder."""
    mount = (mx, my + 40.0, mz)
    for (dx, dz) in ((-16.0, -9.0), (16.0, -9.0), (0.0, 18.0)):
        beam(faces, mount, (mx + dx, my, mz + dz), 6.0, lit(METAL, (dx, 0.0, dz)))

    # The bearing: level, and a whisker below it. The deck stands above the rim,
    # so what a lookout is watching is the ground out beyond the canyon — not
    # the far wall, and certainly not the sky.
    d = _norm((-0.88, -0.04, -0.47))
    eye = tuple(mount[k] - d[k] * 26.0 for k in range(3))
    obj = tuple(mount[k] + d[k] * 96.0 for k in range(3))
    beam(faces, eye, obj, 20.0, lit(METAL, (0.0, 1.0, 0.0)))
    beam(faces, eye, obj, 7.0, METAL_LIT)
    # The objective: a stub of wider barrel with the lens across its end.
    beam(faces, tuple(obj[k] - d[k] * 15.0 for k in range(3)),
         tuple(obj[k] + d[k] * 9.0 for k in range(3)), 30.0,
         lit(METAL, (-0.6, 0.4, -0.7)))
    # The lens itself, a disc built in the plane the tube points along.
    u = _norm((-d[2], 0.0, d[0]))
    v = _norm(cross(d, u))
    face = tuple(obj[k] + d[k] * 11.0 for k in range(3))
    panel(faces, [tuple(face[i] + u[i] * math.cos(TAU * k / 14) * 14.0
                        + v[i] * math.sin(TAU * k / 14) * 14.0 for i in range(3))
                  for k in range(14)], LENS)


# =======================================================================
# The plane
# =======================================================================
#
# Two world points do all the work: where the aircraft is, and the mouth it
# climbed out of. Everything else — its size on the screen, its bearing, the run
# of its contrail — falls out of projecting those two and the camera.

P_AT = (-677.0, 1442.0, 1000.0)    # where it is now: out past the left rim and
                                   # a hundred and fifty above it, in clear sky
P_FROM = (-120.0, 1150.0, 2400.0)  # the reach it climbed out of, a long way back
P_SPAN = 190.0                     # wingspan, in world units

PLANE_BODY = (56, 62, 74)
PLANE_LIT = (120, 130, 148)
CANOPY = (168, 204, 220)
CONTRAIL = (250, 252, 254)

# The silhouette, nose at +x, spanning 84 units tip to tip — `P_SPAN` scales it.
_FUSELAGE = [(46, 0), (30, -4), (-30, -5), (-48, -3), (-52, 0),
             (-48, 3), (-30, 5), (30, 4)]
_WING = [(12, -2), (-28, -42), (-40, -40), (-6, -1)]
_TAIL = [(-40, -1), (-56, -17), (-62, -16), (-46, 1)]
_BELLY = [(40, 0), (22, -2), (-40, -2), (-48, 0), (-40, 2), (22, 2)]
_CANOPY = [(30, 0), (18, -3), (6, -2), (16, 0)]


def _mirror_y(pts):
    return [(x, -y) for (x, y) in pts]


def draw_plane(tf):
    """The aircraft and its trail. Both sit in open sky above every wall in the
    scene, so unlike the tower they are painted over the finished canyon rather
    than filed into its depth sort — and that is what lets the contrail fade out
    with a real alpha instead of a colour pretending to be one."""
    shapes = []
    (fx, fy, _fd) = project(*P_FROM)
    (ax, ay, ad) = project(*P_AT)

    # The bearing is the screen direction of the climb — the same two points that
    # place the aircraft also point it.
    (ux, uy) = (ax - fx, ay - fy)
    run = math.hypot(ux, uy) or 1.0
    (ux, uy) = (ux / run, uy / run)
    s = FOCAL / ad * (P_SPAN / 84.0)

    def place(pts):
        return [(ax + lx * s * ux - ly * s * uy, ay + lx * s * uy + ly * s * ux)
                for (lx, ly) in pts]

    # The contrail, first, so the aircraft sits on its own trail: one quad from
    # just off the tail back down to the bend, widening as it goes because it is
    # older there, and fading to nothing over the same run.
    tail = place([(-58, 0)])[0]
    (px, py) = (-uy, ux)
    (w0, w1) = (2.2 * s, 15.0 * s)
    shapes.append(ramp(tf, [(tail[0] + px * w0, tail[1] + py * w0),
                            (fx + px * w1, fy + py * w1),
                            (fx - px * w1, fy - py * w1),
                            (tail[0] - px * w0, tail[1] - py * w0)],
                       CONTRAIL, CONTRAIL,
                       math.degrees(math.atan2(uy, ux)) + 180.0,
                       near_alpha=125, far_alpha=0))

    # The airframe: wings and tailplanes under the fuselage, the fuselage over
    # them, a lit belly down its length, and the canopy.
    for (shape, colour) in ((_WING, PLANE_BODY), (_mirror_y(_WING), PLANE_BODY),
                            (_TAIL, PLANE_BODY), (_mirror_y(_TAIL), PLANE_BODY),
                            (_FUSELAGE, PLANE_BODY), (_BELLY, PLANE_LIT),
                            (_CANOPY, CANOPY)):
        shapes.append(solid(tf, place(shape), aerial(colour, P_AT[2])))
    return shapes


def build_scene(tf):
    shapes = []
    shapes.append(ramp(tf, [(0, 0), (ART_W, 0), (ART_W, ART_H), (0, ART_H)],
                       SKY_LOW, SKY_TOP, 270.0))
    shapes.append(disc(tf, 800, 96, 82, SUN, alpha=66, sides=32))
    shapes.append(disc(tf, 800, 96, 38, SUN, sides=28))
    faces = []
    build_corridor(faces)
    build_tower(faces)
    build_birds(faces)
    shapes += paint(tf, faces)
    shapes += draw_plane(tf)
    return shapes


# =======================================================================
# The image node (fit-to-box, from tshirt.py)
# =======================================================================


class CanyonFlyover:
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
        return "A Canyon Flyover"


def transform():
    """A red-rock canyon, its firewatch lookout, and a plane climbing out."""
    return CanyonFlyover()
