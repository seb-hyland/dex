"""A protein viewer from a PDB file, on a plane you pan and zoom."""

import math
import random

# ======================================================================
# Colours
# ======================================================================

# CPK element colours for the atom dots.
CPK = {
    "C": (110, 116, 124),
    "N": (72, 112, 196),
    "O": (204, 84, 78),
    "S": (214, 188, 78),
    "P": (222, 148, 78),
    "H": (222, 224, 228),
    "FE": (204, 120, 60),
    "MG": (90, 176, 120),
    "ZN": (130, 132, 176),
    "CA": (90, 168, 150),
    "NA": (150, 110, 190),
    "CL": (108, 176, 110),
}
CPK_DEFAULT = (188, 120, 168)

INK = (58, 62, 70)
FAINT = (120, 126, 136)
FAR = (238, 240, 244)     # the colour the far side fades toward

# ======================================================================
# Tuning
# ======================================================================

PROT_TILT = 0.38          # fixed lean, radians, so it is not seen edge-on
DRAG_SENS = 0.012         # radians of turn per point of horizontal drag
ATOM_R = 2.2              # atom dot radius at the near edge
PICK_RADIUS = 12.0        # how close a click must land to select an atom
PAN_MODE_BOX = (110.0, 22.0)   # the rotate/pan toggle in the corner

# The structure is drawn once at this size, then magnified from there.
VIEW_SIZE = 900.0


# ======================================================================
# Draw helpers
# ======================================================================


def _abs():
    return dex.DrawConstraints(
        pos=dex.ScreenPos.new(0.0, 0.0), x=None, y=None, wrap=None, should_clip=False
    )


def _box(x, y, w, h):
    return dex.DrawConstraints(
        pos=dex.ScreenPos.new(x, y),
        x=dex.AxisConstraint.Exactly(w),
        y=dex.AxisConstraint.Exactly(h),
        wrap=None, should_clip=False,
    )


def _text(ctx, text, x, y, size, rgb):
    label = dex.Label.new(text)
    label.font = dex.Font.proportional(size)
    label.color = dex.Color.rgb(*rgb)
    ctx.draw_node(
        label,
        dex.DrawConstraints(
            pos=dex.ScreenPos.new(x, y), x=None, y=None, wrap=None, should_clip=False
        ),
    )


def _line(ctx, pts, rgb, width):
    ctx.draw_node(dex.Path.polyline([dex.Vector.new(x, y) for (x, y) in pts],
                                    dex.Stroke.new(width, dex.Color.rgb(*rgb))), _abs())


def _polygon(ctx, pts, rgb):
    ctx.draw_node(dex.Path.polygon([dex.Vector.new(x, y) for (x, y) in pts],
                                   dex.Color.rgb(*rgb), dex.Stroke.none()), _abs())


def lerp_rgb(a, b, t):
    t = max(0.0, min(t, 1.0))
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def hsv_rgb(h, s, v):
    i = int(h * 6.0)
    f = h * 6.0 - i
    p, q, t = v * (1 - s), v * (1 - s * f), v * (1 - s * (1 - f))
    (r, g, b) = [(v, t, p), (q, v, p), (p, v, t), (p, q, v), (t, p, v), (v, p, q)][i % 6]
    return (int(r * 255), int(g * 255), int(b * 255))


def octagon(cx, cy, r):
    return [
        (cx + r * math.cos(k * math.pi / 4.0), cy + r * math.sin(k * math.pi / 4.0))
        for k in range(8)
    ]


def circle_pts(cx, cy, r, n=48):
    return [(cx + r * math.cos(2 * math.pi * k / n),
             cy + r * math.sin(2 * math.pi * k / n)) for k in range(n)]


# ======================================================================
# PDB parsing
# ======================================================================


def _f(line, a, b):
    try:
        return float(line[a:b])
    except (ValueError, IndexError):
        return None


def parse_pdb(text):
    """`(title, chain_order, ca_chains, atoms)` from a PDB's first model.

    `ca_chains` is the CA trace per chain (for the backbone); `atoms` is every
    atom as `(element, chain, resname, resseq, name, (x, y, z))` — what the
    per-atom rendering and click-to-inspect need. Water is dropped.
    """
    chains, order, atoms = {}, [], []
    title_parts, header, ended = [], "", False
    for line in text.splitlines():
        rec = line[:6].strip()
        if rec == "TITLE":
            title_parts.append(line[10:80].strip())
        elif rec == "HEADER":
            header = line[10:50].strip()
        elif rec == "ENDMDL":
            ended = True
        elif rec in ("ATOM", "HETATM") and not ended:
            if rec == "HETATM" and line[17:20].strip() == "HOH":
                continue
            (x, y, z) = (_f(line, 30, 38), _f(line, 38, 46), _f(line, 46, 54))
            if x is None:
                continue
            name = line[12:16].strip()
            resname = line[17:20].strip()
            ch = line[21:22] or " "
            resseq = line[22:26].strip()
            elem = (line[76:78].strip() or name[:2] or name[:1]).strip().upper()
            atoms.append((elem, ch, resname, resseq, name, (x, y, z)))
            if rec == "ATOM" and name == "CA":
                if ch not in chains:
                    chains[ch] = []
                    order.append(ch)
                chains[ch].append((x, y, z))
    title = " ".join(p for p in title_parts if p) or header or "structure"
    return title, order, chains, atoms


# ======================================================================
# Atomic model (an electron-density cloud of one element)
# ======================================================================

ELEMENT_Z = {
    "H": 1, "C": 6, "N": 7, "O": 8, "NA": 11, "MG": 12, "P": 15, "S": 16,
    "CL": 17, "K": 19, "CA": 20, "MN": 25, "FE": 26, "CO": 27, "NI": 28,
    "CU": 29, "ZN": 30, "SE": 34, "MO": 42,
}
ELEMENT_NAME = {
    "H": "Hydrogen", "C": "Carbon", "N": "Nitrogen", "O": "Oxygen",
    "NA": "Sodium", "MG": "Magnesium", "P": "Phosphorus", "S": "Sulfur",
    "CL": "Chlorine", "K": "Potassium", "CA": "Calcium", "MN": "Manganese",
    "FE": "Iron", "CO": "Cobalt", "NI": "Nickel", "CU": "Copper",
    "ZN": "Zinc", "SE": "Selenium", "MO": "Molybdenum",
}
SHELL_CAPS = [2, 8, 18, 32, 32, 18, 8]


def electron_shells(z):
    """Electrons per shell for atomic number `z`, Bohr-style (2, 8, 18, …)."""
    out, rem = [], z
    for cap in SHELL_CAPS:
        if rem <= 0:
            break
        out.append(min(cap, rem))
        rem -= cap
    return out


def atom_cloud(z, max_points=900):
    """A 3D electron-density point cloud for atomic number `z`.

    One fuzzy shell per Bohr shell — points scattered on a sphere at the shell's
    radius with a Gaussian radial spread, as many as the shell has electrons.
    Seeded by `z`, so the cloud is stable frame to frame (no shimmer). Radii are
    normalised to ~1 and scaled to the box at draw time.
    """
    shells = electron_shells(z) or [1]
    total = sum(shells) or 1
    n = len(shells)
    rng = random.Random(z * 2654435761 & 0xFFFFFFFF)
    pts = []
    for (i, count) in enumerate(shells):
        r0 = (i + 1) / n
        k = max(10, int(max_points * count / total))
        for _ in range(k):
            u = rng.uniform(-1.0, 1.0)
            th = rng.uniform(0.0, 2.0 * math.pi)
            s = math.sqrt(max(0.0, 1.0 - u * u))
            rr = r0 * (1.0 + rng.gauss(0.0, 0.11))
            pts.append((s * math.cos(th) * rr, s * math.sin(th) * rr, u * rr, i))
    return pts, n


def shell_color(i, n):
    """Inner shells blue, outer shells warm — a simple density gradient."""
    return lerp_rgb((70, 110, 198), (206, 132, 96), i / max(n - 1, 1))


def draw_cloud(ctx, cx, cy, R, points, n_shells, yaw, tilt, dot=1.6):
    """Project the cloud with the viewer's yaw/tilt, depth-sort, fade the far
    side — the same machinery the protein uses, on one atom's electrons."""
    (ca, sa) = (math.cos(yaw), math.sin(yaw))
    (cb, sb) = (math.cos(tilt), math.sin(tilt))
    proj = []
    for (x, y, z, sh) in points:
        dx, dz = x * ca + z * sa, -x * sa + z * ca
        dy, dz = y * cb - dz * sb, y * sb + dz * cb
        proj.append((cx + dx * R, cy - dy * R, dz, sh))
    proj.sort(key=lambda p: p[2])
    zlo = proj[0][2]
    span = (proj[-1][2] - zlo) or 1.0
    _polygon(ctx, circle_pts(cx, cy, max(4.0, R * 0.06), 28), (66, 70, 78))
    for (sx, sy, dz, sh) in proj:
        t = (dz - zlo) / span
        col = lerp_rgb(FAR, shell_color(sh, n_shells), 0.25 + 0.7 * t)
        _polygon(ctx, octagon(sx, sy, dot * (0.7 + 0.8 * t)), col)


class AtomModel:
    """A single atom as a rotatable 3D electron-density cloud — driven by the
    same drag-to-rotate machinery as the protein, and pushable fullscreen."""

    def __init__(self, element, z, sensor):
        self.element = element
        self.z = z
        self.sensor = sensor
        self.points, self.n_shells = atom_cloud(z)
        self.yaw, self.tilt = 0.6, 0.32

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else 320.0
        h = base.y.provided_value() if base.y is not None else 320.0
        if not math.isfinite(w):
            w = 320.0
        if not math.isfinite(h):
            h = 320.0
        ws = ctx.node.workspace
        ctx.draw_node(self.sensor, _box(base.pos.x, base.pos.y, w, h))
        drag = ws.send_request(self.sensor, dex.WasDragged())
        if drag is not None:
            self.yaw += drag.x * DRAG_SENS
            self.tilt = max(-1.4, min(1.4, self.tilt + drag.y * DRAG_SENS))
        cx, cy = base.pos.x + w / 2.0, base.pos.y + h / 2.0
        R = min(w, h) / 2.0 - 18.0
        draw_cloud(ctx, cx, cy, R, self.points, self.n_shells, self.yaw, self.tilt,
                   dot=max(1.6, R * 0.012))
        _text(ctx, "%s  ·  %s  ·  Z %d"
              % (ELEMENT_NAME.get(self.element, self.element), self.element, self.z),
              base.pos.x + 10.0, base.pos.y + 8.0, 12.0, INK)
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))

    def type_name(self):
        return "%s atom" % (ELEMENT_NAME.get(self.element, self.element))

    def owned_nodes(self):
        return [self.sensor]

    def on_delete(self, ctx):
        ctx.workspace.delete_node(self.sensor)

    def build_inspector(self, ctx):
        return None


# ======================================================================
# The viewer
# ======================================================================


class Protein:
    """Every atom as a depth-sorted CPK dot over the backbone trace.

    Drag rotates (yaw + pitch) or pans, per the corner toggle. Click an atom to
    select it: it is ringed, and a panel shows its expanded identity, an
    electron-density model of its element, and an Open-Fullscreen button that
    pushes that model over the whole view.
    """

    def __init__(self, title, order, chains, atoms, sensor, mode_sensor, model_button):
        self.title = title
        self.order = list(order)
        self.chains = {c: list(p) for (c, p) in chains.items()}
        self.atoms = list(atoms)   # (element, chain, resname, resseq, name, (x,y,z))
        self.sensor = sensor
        self.mode_sensor = mode_sensor
        self.model_button = model_button
        self.yaw = 0.0
        self.tilt = PROT_TILT
        self.pan_x = 0.0
        self.pan_y = 0.0
        self.pan_mode = False
        self.selected = None       # index into self.atoms
        self.atom_model_uid = None
        self._model_element = None
        self._prev_elem = None     # cached preview cloud, keyed by element
        self._prev_pts, self._prev_n = None, 0
        pts = [a[5] for a in self.atoms] or [p for c in self.chains.values() for p in c]
        if pts:
            self.center = tuple(sum(p[i] for p in pts) / len(pts) for i in range(3))
            self.radius = max(1.0, max(math.dist(p, self.center) for p in pts))
        else:
            self.center, self.radius = (0.0, 0.0, 0.0), 1.0
        self.ink = {c: hsv_rgb((i / max(len(order), 1)) % 1.0, 0.36, 0.62)
                    for (i, c) in enumerate(order)}

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        h = base.y.provided_value() if base.y is not None else None
        if w is None or h is None:
            return dex.DrawResult.Complete(region=None)
        ws = ctx.node.workspace
        ctx.draw_node(self.sensor, _box(base.pos.x, base.pos.y, w, h))
        drag = ws.send_request(self.sensor, dex.WasDragged())
        if drag is not None:
            if self.pan_mode:
                self.pan_x += drag.x
                self.pan_y += drag.y
            else:
                self.yaw += drag.x * DRAG_SENS
                self.tilt = max(-1.4, min(1.4, self.tilt + drag.y * DRAG_SENS))

        avail = min(w, h) / 2.0 - 12.0
        if avail <= 0.0 or not self.atoms:
            return dex.DrawResult.Complete(
                region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))
        scale = avail / self.radius
        ox = base.pos.x + w / 2.0 + self.pan_x
        oy = base.pos.y + h / 2.0 + self.pan_y
        (ca, sa) = (math.cos(self.yaw), math.sin(self.yaw))
        (cb, sb) = (math.cos(self.tilt), math.sin(self.tilt))
        (cx, cy, cz) = self.center

        def project(p):
            dx, dy, dz = p[0] - cx, p[1] - cy, p[2] - cz
            dx, dz = dx * ca + dz * sa, -dx * sa + dz * ca
            dy, dz = dy * cb - dz * sb, dy * sb + dz * cb
            return (ox + dx * scale, oy - dy * scale, dz)

        proj_atoms = [project(a[5]) for a in self.atoms]

        if ws.send_request(self.sensor, dex.TakeClicked()):
            pos = ws.send_request(self.sensor, dex.PointerPos())
            if pos is not None:
                best, best_d = None, PICK_RADIUS
                for (i, q) in enumerate(proj_atoms):
                    d = math.hypot(q[0] - pos.x, q[1] - pos.y)
                    if d < best_d:
                        best_d, best = d, i
                self.selected = best

        draws = []
        for ch in self.order:
            ink = self.ink[ch]
            proj = [project(p) for p in self.chains[ch]]
            for i in range(len(proj) - 1):
                (a, b) = (proj[i], proj[i + 1])
                draws.append(((a[2] + b[2]) / 2.0, "line", (a, b, ink)))
        for (i, q) in enumerate(proj_atoms):
            draws.append((q[2], "atom", (q, CPK.get(self.atoms[i][0], CPK_DEFAULT))))
        zlo = min(d[0] for d in draws)
        span = (max(d[0] for d in draws) - zlo) or 1.0
        draws.sort(key=lambda d: d[0])
        for (z, kind, payload) in draws:
            t = (z - zlo) / span
            if kind == "line":
                (a, b, ink) = payload
                shade = lerp_rgb(lerp_rgb(ink, FAR, 0.72), ink, t)
                _line(ctx, [(a[0], a[1]), (b[0], b[1])], shade, 1.6 * (0.5 + 0.7 * t))
            else:
                (q, ink) = payload
                shade = lerp_rgb(lerp_rgb(ink, FAR, 0.55), ink, t)
                _polygon(ctx, octagon(q[0], q[1], ATOM_R * (0.55 + 0.7 * t)), shade)

        if self.selected is not None and self.selected < len(proj_atoms):
            (sx, sy, _) = proj_atoms[self.selected]
            ring = octagon(sx, sy, 6.5)
            _line(ctx, ring + [ring[0]], (250, 176, 40), 1.8)
            self._atom_panel(ctx, ws, base.pos.x + 8.0, base.pos.y + 8.0)

        # The rotate/pan toggle, top-right, drawn last so it takes its own clicks.
        (mbw, mbh) = PAN_MODE_BOX
        mx, my = base.pos.x + w - mbw - 8.0, base.pos.y + 8.0
        _polygon(ctx, [(mx, my), (mx + mbw, my), (mx + mbw, my + mbh), (mx, my + mbh)],
                 (238, 240, 244))
        _text(ctx, "Mode: Pan" if self.pan_mode else "Mode: Rotate",
              mx + 8.0, my + 4.0, 10.0, INK)
        ctx.draw_node(self.mode_sensor, _box(mx, my, mbw, mbh))
        if ws.send_request(self.mode_sensor, dex.TakeClicked()):
            self.pan_mode = not self.pan_mode

        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))

    def _atom_panel(self, ctx, ws, x, y):
        """Expanded identity + an electron-density model of the atom's element,
        with a button to push that model fullscreen."""
        (elem, ch, resname, resseq, name, coord) = self.atoms[self.selected]
        z = ELEMENT_Z.get(elem, 0)
        pw, ph = 176.0, 168.0
        _polygon(ctx, [(x, y), (x + pw, y), (x + pw, y + ph), (x, y + ph)], (255, 255, 255))
        _text(ctx, "%s %s%s" % (resname or "?", ch.strip() or "?", resseq),
              x + 8.0, y + 6.0, 11.0, INK)
        _text(ctx, "atom %s" % name, x + 8.0, y + 22.0, 10.0, (90, 96, 104))
        _text(ctx, "%s (%s, Z %d)" % (ELEMENT_NAME.get(elem, elem), elem, z),
              x + 8.0, y + 36.0, 10.0, (90, 96, 104))
        _text(ctx, "x %.2f  y %.2f  z %.2f" % coord, x + 8.0, y + 50.0, 9.0, FAINT)
        # Electron-density preview (static; the fullscreen model rotates).
        if elem != self._prev_elem:
            self._prev_pts, self._prev_n = atom_cloud(z, 220)
            self._prev_elem = elem
        draw_cloud(ctx, x + pw / 2.0, y + 96.0, 30.0, self._prev_pts, self._prev_n,
                   0.6, 0.32, dot=1.4)
        # Open-Fullscreen button (a rotatable 3D model).
        ctx.draw_node(self.model_button, _box(x + 8.0, y + ph - 26.0, pw - 16.0, 22.0))
        if ws.send_request(self.model_button, dex.TakeClicked()):
            handle = ws.action_handle()
            if self.atom_model_uid is None or self._model_element != elem:
                if self.atom_model_uid is not None:
                    handle.delete_node(self.atom_model_uid)
                msensor = handle.insert_node_dyn(dex.InteractionBox.sensing(False, False, True))
                self.atom_model_uid = handle.insert_node_dyn(AtomModel(elem, z, msensor))
                self._model_element = elem
            ws.submit_action(ws.root(), dex.PushOverride(node=self.atom_model_uid),
                             "Atom model fullscreen")

    # -- messages --------------------------------------------------------

    def type_name(self):
        return "A PDB Viewer"

    def owned_nodes(self):
        out = [self.sensor, self.mode_sensor, self.model_button]
        if self.atom_model_uid is not None:
            out.append(self.atom_model_uid)
        return out

    def on_delete(self, ctx):
        ctx.workspace.delete_node(self.sensor)
        ctx.workspace.delete_node(self.mode_sensor)
        ctx.workspace.delete_node(self.model_button)
        if self.atom_model_uid is not None:
            ctx.workspace.delete_node(self.atom_model_uid)

    def build_inspector(self, ctx):
        return None


# ======================================================================
# The canvas the structure sits on
# ======================================================================


def _on_canvas(ws, protein):
    """Put `protein` on a plane of its own and return the canvas.

    Named after what it holds: a plane is a means, and "A Canvas" is the wrong
    answer to what the inspector and the breadcrumb trail are asking.
    """
    canvas = dex.Canvas.build(ws)
    protein_uid = ws.insert_node_dyn(protein)
    item = dex.StaticCanvasItem.build(
        ws, protein_uid,
        dex.Vector.new(0.0, 0.0),
        dex.Vector.new(VIEW_SIZE, VIEW_SIZE),
    )
    ws.submit_action(canvas, dex.AdoptCanvasNode(item, dex.Layer.midground()),
                     "Placed the structure")
    ws.submit_action(canvas, dex.NameCanvas(name="Structure"), "Named the plane")
    return canvas


# ======================================================================
# Build and transform
# ======================================================================


def build(ws, pdb_text):
    """Parse `pdb_text` and build the viewer: the structure on a zoomable plane,
    turned by dragging it, with click-to-inspect atoms."""
    (title, order, chains, atoms) = parse_pdb(pdb_text)
    # The main view senses clicks (pick an atom) and drags (rotate / pan).
    sensor = ws.insert_node_dyn(dex.InteractionBox.sensing(False, True, True))
    mode_sensor = ws.insert_node_dyn(dex.InteractionBox.sensing(False, True, False))
    model_button = dex.Button.build(ws, dex.Label.new("Open Fullscreen"))
    protein = Protein(title, order, chains, atoms, sensor, mode_sensor, model_button)
    return _on_canvas(ws, protein)


def transform():
    """The drag-to-spin, click-to-inspect viewer of the wired `pdb_data` string."""
    text = pdb_data if "pdb_data" in globals() else None
    if not text:
        for value in globals().values():
            if isinstance(value, str) and "\nATOM" in ("\n" + value):
                text = value
                break
    if not text:
        raise ValueError("wire a PDB (.pdb) string into this transform as `pdb_data`")
    return build(dex.ws, text)
