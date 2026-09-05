"""One taxonomic tree that drills all the way down to structure.

Three views, joined into a single object you explore by clicking inward:

  1. **The tree** — a circular phylogeny from the GTDB-style table (branches
     tinted by phylum, a labelled clade ring, one clickable marker per tip).
  2. **Click a tip → its genome.** The tip's accession is fetched from NCBI on a
     background task; while it is in flight the inspector shows "Loading…", then
     swaps itself for the Genome Explorer of that assembly. It carries a
     fullscreen button, because a genome does not fit in a popup.
  3. **Click a gene → its structure.** The gene's inspector shows its record
     and, if the CDS carries a UniProt cross-reference, fetches the AlphaFold
     model for it — again on a task, again with fullscreen.

Nothing is precomputed and nothing blocks: every fetch runs through `dex.spawn`,
which hands a Python callable to a background thread. The callable owns a
`WorkspaceActionHandle` (which is `Send`) and commits its result by queuing an
action — `insert_node_at_dyn(uid, node)` — that swaps the pending placeholder for
the finished view. The main loop drains that action like any other, so a
finished fetch appears on the next frame. Errors become an `ErrorLayout` in the
same slot rather than vanishing.

The join keys, both carried in the data itself:
  * **tip → genome**: the table's `key` column (`GB_GCA_…` / `RS_GCF_…`), the
    GTDB assembly accession, prefix stripped, fetched via the NCBI Datasets API.
  * **gene → structure**: the UniProt accession from the CDS `/db_xref=
    "UniProtKB/…"`, fetched from the AlphaFold model archive.

Scope note: this keeps the tree to branches + clade ring + tips so the drill-down
is the story. The annotation rings from `circos_table.py` layer straight back in
if you want them. And one clickable node per tip is a lot of nodes on a full
GTDB tree — fine for a clade, heavy for ten thousand tips; subsample the table
for the whole tree, or make only a chosen rank's tips inspectable.
"""

import io
import math
import random
import re
import urllib.error
import urllib.parse
import urllib.request
import zipfile

# ======================================================================
# Networking — the two joins, as URLs
# ======================================================================

NCBI_DATASETS = (
    "https://api.ncbi.nlm.nih.gov/datasets/v2alpha/genome/accession/"
    "{acc}/download?include_annotation_type=GENOME_GBFF"
)
ALPHAFOLD = "https://alphafold.ebi.ac.uk/files/AF-{uni}-F1-model_v4.pdb"
UA = {"User-Agent": "dex-superphylogeny/0.1 (research use)"}
TIMEOUT = 60.0


def strip_gtdb(key):
    """`GB_GCA_041494275.1` / `RS_GCF_…` → the bare assembly accession."""
    for prefix in ("GB_", "RS_"):
        if key.startswith(prefix):
            return key[len(prefix):]
    return key


def fetch_gbff(gtdb_key):
    """The genomic GenBank flat file for a GTDB accession, from NCBI Datasets.

    The Datasets endpoint answers with a zip; the gbff is the `*.gbff` member
    inside `ncbi_dataset/data/<acc>/`.
    """
    acc = strip_gtdb(gtdb_key)
    req = urllib.request.Request(NCBI_DATASETS.format(acc=acc), headers=UA)
    raw = urllib.request.urlopen(req, timeout=TIMEOUT).read()
    # NCBI answers errors (rate limits, withdrawn/absent accessions) with a JSON
    # or HTML body, not a zip — which would otherwise surface as a bare
    # BadZipFile. Detect the zip magic and report the real message instead.
    if raw[:4] != b"PK\x03\x04":
        snippet = raw[:240].decode("utf-8", "replace").strip().replace("\n", " ")
        raise ValueError("NCBI returned no genome zip for %s (maybe rate-limited "
                         "or unavailable): %s" % (acc, snippet or "empty response"))
    with zipfile.ZipFile(io.BytesIO(raw)) as zf:
        member = next((n for n in zf.namelist() if n.endswith(".gbff")), None)
        if member is None:
            raise ValueError("no .gbff in the NCBI download for %s" % acc)
        return zf.read(member).decode("utf-8", "replace")


def fetch_alphafold(uniprot):
    """The AlphaFold predicted structure (PDB) for a UniProt accession."""
    req = urllib.request.Request(ALPHAFOLD.format(uni=uniprot), headers=UA)
    return urllib.request.urlopen(req, timeout=TIMEOUT).read().decode("utf-8", "replace")


ESMFOLD = "https://api.esmatlas.com/foldSequence/v1/pdb/"
ESMFOLD_MAX = 400  # the public ESMFold endpoint refuses long sequences


def fetch_esmfold(sequence):
    """Fold an amino-acid `sequence` to a PDB with ESMFold.

    AlphaFold only covers UniProt, so MAG proteins (most of a metagenome) are not
    in it. ESMFold predicts straight from sequence, so any CDS with a
    `/translation` can get a structure — at the cost of a live fold each time.
    """
    req = urllib.request.Request(
        ESMFOLD, data=sequence.encode(),
        headers=dict(UA, **{"Content-Type": "text/plain"}), method="POST")
    return urllib.request.urlopen(req, timeout=120.0).read().decode("utf-8", "replace")


UNIPROT_SEARCH = (
    "https://rest.uniprot.org/uniprotkb/search?query={q}&format=list&size=1"
)
# protein_id -> UniProt accession ("" for a miss), so opening many genes of one
# genome resolves each id at most once. Shared across the background tasks; dict
# reads/writes are atomic enough under the GIL for this.
_UNIPROT_CACHE = {}


def resolve_uniprot(protein_id):
    """A UniProt accession for a RefSeq/GenBank `protein_id`, via UniProtKB.

    The AlphaFold archive is keyed by UniProt, but a CDS usually carries only a
    `/protein_id` (WP_…/NP_…). This maps one to the other so any annotated CDS —
    not just those with a UniProtKB `/db_xref` — can reach a model. A miss is
    cached as "" too, so it is not retried.
    """
    if not protein_id:
        return ""
    if protein_id in _UNIPROT_CACHE:
        return _UNIPROT_CACHE[protein_id]
    acc = ""
    try:
        bare = protein_id.split(".")[0]  # UniProt xrefs are unversioned
        url = UNIPROT_SEARCH.format(q=urllib.parse.quote("xref:" + bare))
        body = urllib.request.urlopen(
            urllib.request.Request(url, headers=UA), timeout=TIMEOUT
        ).read().decode("utf-8", "replace").strip()
        acc = body.splitlines()[0].strip() if body else ""
    except Exception:  # noqa: BLE001 - a lookup failure is just a miss
        acc = ""
    _UNIPROT_CACHE[protein_id] = acc
    return acc


# ======================================================================
# Async: pending now, result (or error) later
# ======================================================================


def _pending(message):
    label = dex.Label.new(message)
    label.singleline = False
    label.color = dex.Color.rgb(120, 126, 136)
    return label


def _note(message):
    """A quiet, non-error explanatory label (e.g. no structure available)."""
    label = dex.Label.new(message)
    label.singleline = False
    label.color = dex.Color.rgb(120, 126, 136)
    return label


def async_slot(ws, produce):
    """A node id showing "Loading…" now, filled by `produce(ws)` off-thread.

    `produce` runs on a background thread (via `dex.spawn`), builds its result
    through the `Send` action handle `ws`, and returns the node to seat. Whatever
    it raises becomes an `ErrorLayout` in the same slot. Either way the swap is
    one queued action, applied on the next frame.
    """
    uid = dex.NodeUid.mint()
    ws.insert_node_at_dyn(uid, _pending("Loading…"))

    def worker():
        try:
            node = produce(ws)
        except Exception as exc:  # noqa: BLE001 - a task must not die silently
            node = dex.ErrorLayout.message("%s: %s" % (type(exc).__name__, exc))
        ws.insert_node_at_dyn(uid, node)

    dex.spawn(worker)
    return uid


# ======================================================================
# Fullscreen wrapper: a thumbnail in the popup, big on demand
# ======================================================================

FS_BTN_H = 32.0
THUMB_W = 380.0
THUMB_H = 280.0
INK = (58, 62, 70)
FAINT = (120, 126, 136)


class Framed:
    """A preview with an "Open Fullscreen" button beneath it — the same `Button`
    the canvas uses — which pushes `body` as a Desktops override (`PushOverride`)
    to fill the content area.

    `body` and the button are OWNED here, but `Framed` itself is owned by a stable
    parent (`Tip`/`Gene`), not by the inspector — so opening fullscreen (which
    closes the inspector) does not take the node with it.
    """

    def __init__(self, body, button):
        self.body = body        # uid of the node to preview / fullscreen
        self.button = button    # uid of the "Open Fullscreen" Button

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else THUMB_W
        h = base.y.provided_value() if base.y is not None else THUMB_H + FS_BTN_H
        if not math.isfinite(w):
            w = THUMB_W
        if not math.isfinite(h):
            h = THUMB_H + FS_BTN_H
        w = max(w, THUMB_W)
        h = max(h, THUMB_H + FS_BTN_H)
        (x0, y0) = (base.pos.x, base.pos.y)

        ctx.draw_node(self.body, _box(x0, y0, w, h - FS_BTN_H))
        ctx.draw_node(self.button,
                      _box(x0, y0 + h - FS_BTN_H + 4.0, min(200.0, w), FS_BTN_H - 8.0))
        if ctx.node.workspace.send_request(self.button, dex.TakeClicked()):
            root = ctx.node.workspace.root()
            ctx.node.workspace.submit_action(
                root, dex.PushOverride(node=self.body), "Opened fullscreen"
            )
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h))
        )

    def type_name(self):
        return "Preview"

    def owned_nodes(self):
        return [self.body, self.button]

    def on_delete(self, ctx):
        ctx.workspace.delete_node(self.body)
        ctx.workspace.delete_node(self.button)

    def build_inspector(self, ctx):
        return None


def framed(ws, body_uid):
    """Wrap an already-inserted `body_uid` with an Open-Fullscreen button."""
    button = dex.Button.build(ws, dex.Label.new("Open Fullscreen"))
    return Framed(body_uid, button)


class Ref:
    """Draws another node by id WITHOUT owning it.

    `build_inspector` returns one of these so the inspector shows a persistent
    result; when the inspector closes and deletes this ref, the result — owned by
    a stable parent (`Tip`/`Gene`) — is untouched, and so survives to be pushed
    fullscreen.
    """

    def __init__(self, target):
        self.target = target

    def draw(self, ctx):
        return ctx.draw_node(self.target, ctx.constraints)

    def type_name(self):
        return "Result"

    def owned_nodes(self):
        return []

    def on_delete(self, ctx):
        pass

    def build_inspector(self, ctx):
        return None


class FoldPrompt:
    """A button that folds a sequence with ESMFold on demand.

    ESMFold predicts from sequence — the only route to a structure for a MAG
    protein AlphaFold has never seen — but each fold is a live, slow computation.
    So it does not run on inspect: this offers the button, and only a click
    replaces it (in place, at its own id) with the pending fold and then the
    structure.
    """

    def __init__(self, seq, button):
        self.seq = seq
        self.button = button

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else THUMB_W
        h = base.y.provided_value() if base.y is not None else 58.0
        if not math.isfinite(w):
            w = THUMB_W
        if not math.isfinite(h):
            h = 58.0
        w = max(w, THUMB_W)
        (x0, y0) = (base.pos.x, base.pos.y)
        _text(ctx, "No known structure — fold %d aa on demand:" % len(self.seq),
              x0, y0 + 4.0, 11.0, FAINT)
        ctx.draw_node(self.button, _box(x0, y0 + 26.0, min(240.0, w), 24.0))
        if ctx.node.workspace.send_request(self.button, dex.TakeClicked()):
            me = ctx.node.id
            ws = ctx.node.workspace.action_handle()
            seq = self.seq
            ws.delete_node(self.button)
            ws.insert_node_at_dyn(me, _pending("Folding with ESMFold — this can take a while…"))

            def worker():
                try:
                    node = framed(ws, ws.insert_node_dyn(build_protein(ws, fetch_esmfold(seq))))
                except Exception as exc:  # noqa: BLE001
                    node = dex.ErrorLayout.message("%s: %s" % (type(exc).__name__, exc))
                ws.insert_node_at_dyn(me, node)

            dex.spawn(worker)
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, max(h, 58.0))))

    def type_name(self):
        return "Fold on demand"

    def owned_nodes(self):
        return [self.button]

    def on_delete(self, ctx):
        ctx.workspace.delete_node(self.button)

    def build_inspector(self, ctx):
        return None


# ======================================================================
# Shared draw helpers
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


def hsv_rgb(h, s, v):
    i = int(h * 6.0)
    f = h * 6.0 - i
    p, q, t = v * (1 - s), v * (1 - s * f), v * (1 - s * (1 - f))
    (r, g, b) = [(v, t, p), (q, v, p), (p, v, t), (p, q, v), (t, p, v), (v, p, q)][i % 6]
    return (int(r * 255), int(g * 255), int(b * 255))


def lerp_rgb(a, b, t):
    t = max(0.0, min(t, 1.0))
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def octagon(cx, cy, r):
    return [
        (cx + r * math.cos(k * math.pi / 4.0), cy + r * math.sin(k * math.pi / 4.0))
        for k in range(8)
    ]


# ======================================================================
# GenBank parsing (shared with genome_explorer.py)
# ======================================================================


def _f(line, a, b):
    try:
        return float(line[a:b])
    except (ValueError, IndexError):
        return None


def parse_location(loc):
    strand = -1 if "complement" in loc else 1
    nums = re.findall(r"\d+", loc)
    if not nums:
        return None
    ints = [int(n) for n in nums]
    return (min(ints), max(ints), strand)


def parse_genbank(text):
    """Records of name/length/definition/features, first model only, no water."""
    records = []
    cur = None
    lines = text.splitlines()
    i, n = 0, len(lines)
    while i < n:
        line = lines[i]
        rec = line[:6].strip()
        if line.startswith("LOCUS"):
            cur = {"name": "", "length": 0, "definition": "", "features": []}
            parts = line.split()
            if len(parts) >= 2:
                cur["name"] = parts[1]
            for (j, p) in enumerate(parts):
                if p == "bp" and j > 0:
                    try:
                        cur["length"] = int(parts[j - 1].replace(",", ""))
                    except ValueError:
                        pass
            records.append(cur)
            i += 1
        elif cur is not None and line.startswith("DEFINITION"):
            cur["definition"] = line[12:].strip()
            i += 1
            while i < n and lines[i].startswith(" " * 12):
                cur["definition"] += " " + lines[i].strip()
                i += 1
        elif cur is not None and line.startswith("FEATURES"):
            i = _parse_features(lines, i + 1, n, cur)
        else:
            i += 1
    return records


def _parse_features(lines, i, n, rec):
    while i < n:
        fl = lines[i]
        if fl.strip() and not fl[:1].isspace():
            break
        if fl.strip() and not fl[:6].isspace():
            key = fl[5:21].strip()
            loc = fl[21:].strip()
            quals = {}
            i += 1
            last = None
            while i < n and lines[i].strip() and lines[i][:6].isspace():
                s = lines[i].strip()
                if s.startswith("/"):
                    if "=" in s:
                        (k, v) = s[1:].split("=", 1)
                        v = v.strip().strip('"')
                        if k == "translation":
                            quals[k] = v.replace(" ", "")  # kept, for ESMFold
                        elif k == "db_xref":
                            quals.setdefault("db_xref", []).append(v)
                        else:
                            quals[k] = v
                        last = k
                    else:
                        quals[s[1:]] = True
                        last = s[1:]
                elif last == "translation":
                    quals["translation"] = quals.get("translation", "") + s.strip('"').replace(" ", "")
                elif last == "db_xref" and quals.get("db_xref"):
                    quals["db_xref"][-1] += s.strip('"')
                elif last and isinstance(quals.get(last), str):
                    joiner = "" if quals[last].endswith("-") else " "
                    quals[last] = quals[last] + joiner + s.strip('"')
                else:
                    loc += s
                i += 1
            rec["features"].append((key, loc, quals))
        else:
            i += 1
    return i


def uniprot_of(quals):
    """The UniProt accession from a CDS's db_xref list, if any."""
    for x in quals.get("db_xref", []):
        if x.startswith("UniProtKB"):
            return x.split(":", 1)[-1]
    return ""


# ======================================================================
# PDB parsing + the drag-to-spin viewer (shared with pdb_viewer.py)
# ======================================================================

CPK = {
    "C": (110, 116, 124), "N": (72, 112, 196), "O": (204, 84, 78),
    "S": (214, 188, 78), "P": (222, 148, 78), "FE": (204, 120, 60),
    "MG": (90, 176, 120), "ZN": (130, 132, 176),
}
CPK_DEFAULT = (188, 120, 168)
FAR = (238, 240, 244)
PROT_TILT = 0.38
DRAG_SENS = 0.012
ATOM_R = 2.2          # atom dot radius at the near edge
PICK_RADIUS = 12.0    # how close a click must land to select an atom


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
# Atomic model (a Bohr diagram of one element)
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


def circle_pts(cx, cy, r, n=48):
    return [(cx + r * math.cos(2 * math.pi * k / n),
             cy + r * math.sin(2 * math.pi * k / n)) for k in range(n)]


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


PAN_MODE_BOX = (110.0, 22.0)   # the rotate/pan toggle in the corner


class Protein:
    """Every atom as a depth-sorted CPK dot over the backbone trace.

    Drag rotates (yaw + pitch) or pans, per the corner toggle. Click an atom to
    select it: it is ringed, and a panel shows its expanded identity, a Bohr
    model of its element, and an Open-Fullscreen button that pushes that model
    over the whole view — the same fullscreen the protein itself has.
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
        """Expanded identity + a Bohr model of the atom's element, with a button
        to push that model fullscreen."""
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


def _line(ctx, pts, rgb, width):
    ctx.draw_node(dex.Path.polyline([dex.Vector.new(x, y) for (x, y) in pts],
                                    dex.Stroke.new(width, dex.Color.rgb(*rgb))), _abs())


def _polygon(ctx, pts, rgb):
    ctx.draw_node(dex.Path.polygon([dex.Vector.new(x, y) for (x, y) in pts],
                                   dex.Color.rgb(*rgb), dex.Stroke.none()), _abs())


def build_protein(ws, pdb_text):
    (title, order, chains, atoms) = parse_pdb(pdb_text)
    # Main view senses clicks (pick an atom) and drags (rotate / pan).
    sensor = ws.insert_node_dyn(dex.InteractionBox.sensing(False, True, True))
    mode_sensor = ws.insert_node_dyn(dex.InteractionBox.sensing(False, True, False))
    model_button = dex.Button.build(ws, dex.Label.new("Open Fullscreen"))
    return Protein(title, order, chains, atoms, sensor, mode_sensor, model_button)


# ======================================================================
# Genome explorer + genes (see genome_explorer.py; genes fetch structures)
# ======================================================================

FEATURE_COLORS = {
    "CDS": (86, 124, 176), "tRNA": (94, 168, 116), "rRNA": (206, 132, 84),
    "ncRNA": (150, 110, 180), "tmRNA": (176, 120, 168), "regulatory": (200, 176, 90),
}
G_MARGIN, G_TITLE, ROW_H, BAND, LINE_HALF = 12.0, 22.0, 40.0, 8.0, 5.0
G_LEGEND = 22.0
TRACK_BG = (240, 242, 246)


def arrow_points(x0, x1, y0, y1, strand):
    head = min((x1 - x0) * 0.5, y1 - y0)
    if x1 - x0 < 3.0 or head < 1.5:
        return [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    ymid = (y0 + y1) / 2.0
    if strand >= 0:
        return [(x0, y0), (x1 - head, y0), (x1, ymid), (x1 - head, y1), (x0, y1)]
    return [(x1, y0), (x0 + head, y0), (x0, ymid), (x0 + head, y1), (x1, y1)]


class Gene:
    """A gene arrow whose inspector shows its record and, if it has a UniProt
    xref, fetches and shows the AlphaFold structure below it."""

    def __init__(self, key, name, locus, product, start, end, strand,
                 uniprot, protein_id, translation):
        self.key = key
        self.name = name
        self.locus = locus
        self.product = product
        self.start = start
        self.end = end
        self.strand = strand
        self.uniprot = uniprot          # from a UniProtKB /db_xref, if present
        self.protein_id = protein_id    # /protein_id, resolved to UniProt on demand
        self.translation = translation  # the AA sequence, for ESMFold fallback
        self.color = FEATURE_COLORS.get(key, (150, 150, 156))
        self._panel = None

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else 0.0
        h = base.y.provided_value() if base.y is not None else 0.0
        if w <= 0.0 or h <= 0.0:
            return dex.DrawResult.Complete(region=None)
        (x0, y0) = (base.pos.x, base.pos.y)
        pts = arrow_points(x0, x0 + w, y0, y0 + h, self.strand)
        _polygon(ctx, pts, self.color)
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))

    def _data_label(self):
        rows = []
        if self.name:
            rows.append("Gene: %s" % self.name)
        if self.locus:
            rows.append("Locus tag: %s" % self.locus)
        rows.append("Type: %s" % self.key)
        if self.product:
            rows.append("Product: %s" % self.product)
        if self.uniprot:
            rows.append("UniProt: %s" % self.uniprot)
        arrow = "→" if self.strand >= 0 else "←"
        rows.append("Location: %d %s %d" % (self.start, arrow, self.end))
        label = dex.Label.new("\n".join(rows))
        label.singleline = False
        label.color = dex.Color.rgb(*INK)
        return label

    def type_name(self):
        return "%s (%s)" % (self.name or self.locus or self.key, self.key)

    def build_inspector(self, ctx):
        # `build_inspector` gets a NodeContext (`.workspace`), not a DrawContext.
        ws = ctx.workspace.action_handle()
        if self._panel is None:
            data_uid = ws.insert_node_dyn(self._data_label())
            # Any CDS with a UniProt xref, a protein_id, or a translation can
            # reach a model: AlphaFold if it's a known UniProt protein, else
            # ESMFold folds the sequence directly (which is what MAG genes need).
            if self.key != "CDS" or not (self.uniprot or self.protein_id or self.translation):
                self._panel = data_uid
            else:
                uni, pid, seq = self.uniprot, self.protein_id, self.translation

                def produce(ws):
                    acc = uni or resolve_uniprot(pid)
                    if acc:
                        try:
                            pdb = fetch_alphafold(acc)
                            return framed(ws, ws.insert_node_dyn(build_protein(ws, pdb)))
                        except urllib.error.HTTPError as e:
                            if e.code != 404:
                                raise
                    # No precomputed model. ESMFold can fold the sequence live, but
                    # that is slow — so offer a button, not an automatic wait.
                    if seq and len(seq) <= ESMFOLD_MAX:
                        button = dex.Button.build(ws, dex.Label.new("Predict structure (ESMFold)"))
                        return FoldPrompt(seq, button)
                    if seq:
                        return _note("No AlphaFold model; %d aa is over the "
                                     "ESMFold limit (%d)." % (len(seq), ESMFOLD_MAX))
                    return _note("No structure available for this gene.")

                struct_uid = async_slot(ws, produce)
                self._panel = ws.insert_node_dyn(
                    dex.VerticalLayout.new([data_uid, struct_uid], 8.0))
        # The panel is owned by this Gene (persistent); the inspector only
        # borrows it through a Ref, so closing the inspector cannot delete it.
        return ws.insert_node_dyn(Ref(self._panel))

    def owned_nodes(self):
        return [self._panel] if self._panel is not None else []

    def on_delete(self, ctx):
        if self._panel is not None:
            ctx.workspace.delete_node(self._panel)


class GenomeExplorer:
    """The wrapped genome map (see genome_explorer.py), genes inspectable."""

    def __init__(self, records, offsets, genes, total_len, types_present):
        self.records = list(records)
        self.offsets = list(offsets)
        self.genes = list(genes)   # [{uid, start, end, strand, key}]
        self.total_len = total_len
        self.types_present = list(types_present)
        defn = (records[0][2] if records else "") or (records[0][0] if records else "genome")
        self.title = "%s — %d bp · %d features" % (defn, total_len, len(genes))

    def draw(self, ctx):
        base = ctx.constraints
        width = base.x.provided_value() if base.x is not None else None
        height = base.y.provided_value() if base.y is not None else None
        if width is None or height is None:
            return dex.DrawResult.Complete(region=None)
        # An inspector measure pass can hand a non-finite axis; don't let it reach int().
        if not math.isfinite(width):
            width = THUMB_W
        if not math.isfinite(height):
            height = THUMB_H
        x0, y0 = base.pos.x + G_MARGIN, base.pos.y + G_MARGIN
        plot_w, plot_h = width - 2 * G_MARGIN, height - 2 * G_MARGIN
        # Truncate the title to the panel width, so it never spills the thumbnail.
        title = self.title
        tfont = dex.Font.proportional(12.0)
        tm = ctx.measure_text(title, tfont, dex.TextWrap.singleline())
        if tm.width > plot_w and len(title) > 4:
            avg = tm.width / len(title)
            keep = max(4, int(plot_w / max(avg, 1.0)) - 1)
            title = title[:keep].rstrip() + "…"
        _text(ctx, title, x0, y0, 12.0, INK)
        top = y0 + G_TITLE
        avail_h = plot_h - G_TITLE - G_LEGEND
        if avail_h < ROW_H or self.total_len <= 0 or plot_w <= 70.0:
            return self._done(base, width, height)
        n_rows = max(1, int(avail_h // ROW_H))
        bp_per_row = max(1, int(math.ceil(self.total_len / n_rows)))
        n_rows = int(math.ceil(self.total_len / bp_per_row))
        track_x = x0 + 60.0
        scale = (plot_w - 60.0) / bp_per_row
        for r in range(n_rows):
            line_y = top + r * ROW_H + BAND + LINE_HALF
            row_len = min(bp_per_row, self.total_len - r * bp_per_row)
            _line(ctx, [(track_x, line_y), (track_x + row_len * scale, line_y)],
                  (168, 174, 184), 1.0)
            _text(ctx, "{:,}".format(r * bp_per_row + 1), x0, line_y - 6.0, 9.0, FAINT)
        for g in self.genes:
            r = g["start"] // bp_per_row
            if r >= n_rows:
                continue
            row_bp0 = r * bp_per_row
            gx0 = track_x + (g["start"] - row_bp0) * scale
            gx1 = track_x + (min(g["end"], row_bp0 + bp_per_row) - row_bp0) * scale
            if gx1 - gx0 < 1.2:
                gx1 = gx0 + 1.2
            line_y = top + r * ROW_H + BAND + LINE_HALF
            if g["strand"] >= 0:
                gy0, gy1 = line_y - LINE_HALF - BAND, line_y - LINE_HALF
            else:
                gy0, gy1 = line_y + LINE_HALF, line_y + LINE_HALF + BAND
            ctx.draw_inspectable_node(g["uid"], _box(gx0, gy0, gx1 - gx0, gy1 - gy0))
        self._legend(ctx, x0, base.pos.y + height - G_MARGIN - G_LEGEND + 4.0, plot_w)
        return self._done(base, width, height)

    def _legend(self, ctx, x, y, width):
        """A swatch and name per feature type present, left to right."""
        font = dex.Font.proportional(10.0)
        wrap = dex.TextWrap.singleline()
        sw = 11.0
        right = x + width
        for key in self.types_present:
            m = ctx.measure_text(key, font, wrap)
            if x + sw + 4.0 + m.width > right:
                break
            _polygon(ctx, [(x, y), (x + sw, y), (x + sw, y + sw), (x, y + sw)],
                     FEATURE_COLORS.get(key, (150, 150, 156)))
            _text(ctx, key, x + sw + 4.0, y + (sw - m.height) / 2.0, 10.0, INK)
            x += sw + 6.0 + m.width + 16.0

    def _done(self, base, w, h):
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))

    def type_name(self):
        return "A Genome Explorer"

    def owned_nodes(self):
        return [g["uid"] for g in self.genes]

    def on_delete(self, ctx):
        for g in self.genes:
            ctx.workspace.delete_node(g["uid"])

    def build_inspector(self, ctx):
        return None


def build_genome_explorer(ws, gbff_text):
    records = parse_genbank(gbff_text)
    genes, offsets, recmeta, types_present, offset = [], [], [], [], 0
    for rec in records:
        length = rec["length"]
        feats = rec["features"]
        if not length:
            ends = [parse_location(l)[1] for (k, l, q) in feats if parse_location(l)]
            length = max(ends) if ends else 0
        offsets.append(offset)
        recmeta.append((rec["name"], length, rec["definition"]))
        for (key, loc, quals) in feats:
            if key not in FEATURE_COLORS:
                continue
            parsed = parse_location(loc)
            if parsed is None:
                continue
            (s, e, strand) = parsed
            gene = Gene(key, quals.get("gene", ""), quals.get("locus_tag", ""),
                        quals.get("product", ""), s, e, strand,
                        uniprot_of(quals), quals.get("protein_id", ""),
                        quals.get("translation", "") if isinstance(quals.get("translation"), str) else "")
            uid = ws.insert_node_dyn(gene)
            genes.append({"uid": uid, "start": offset + s, "end": offset + e,
                          "strand": strand, "key": key})
            if key not in types_present:
                types_present.append(key)
        offset += length
    return GenomeExplorer(recmeta, offsets, genes, offset, types_present)


# ======================================================================
# The tree
# ======================================================================

NODE_COL, PARENT_COL, DEPTH_COL = "node", "parent", "depth"
LEAF_COL, LEAF_ORDER_COL, KEY_COL = "is_leaf", "leaf_order", "key"
CLADE_COL, LABEL_COL = "phylum", "label"

# Column roles and which columns are metadata (never drawn as annotation rings).
META_COLS = {"key", "kind", "n_contigs", "domain"}
STRUCTURAL = {NODE_COL, PARENT_COL, DEPTH_COL, LEAF_COL, LEAF_ORDER_COL,
              LABEL_COL, CLADE_COL, KEY_COL, "distance", "phylum",
              "genome", "structures"}

# Matches circos_table.py: the tree in the middle, annotation rings, clade rim.
OPEN_ANGLE = 0.36
START_ANGLE = -math.pi / 2 + OPEN_ANGLE / 2.0
R_CLADE_OUT, R_CLADE_IN = 1.00, 0.955
R_ANNO_OUT, R_ANNO_IN = 0.945, 0.50
R_LEAF, R_ROOT = 0.48, 0.04
BLOCK_INSET = 0.08
RING_GAP = 0.15
CURVE_TOL = 0.12
BRANCH_WIDTH_AT, BRANCH_WIDTH = 420.0, 1.1
CLADE_FONT, RING_FONT = 11.0, 8.5
CLADE_LABEL_GAP, RING_LABEL_GAP = 7.0, 4.0
LABEL_MIN_RADIUS = 170.0
MIN_CLADE_LABEL_ARC = 26.0
RAMP_LEVELS = 20
PADDING = 8.0
BRANCH_INK = (150, 156, 166)
TRACK_BG = (240, 242, 246)
TIP_DOT = 6.0  # the click box; the drawn dot is a fraction of this


def annotation_columns(names):
    """Annotation columns (those with ':'), grouped and ordered by family."""
    chosen = [n for n in names
              if ":" in n and n not in STRUCTURAL and n not in META_COLS]
    families, grouped = [], {}
    for name in chosen:
        family = name.split(":", 1)[1]
        if family not in grouped:
            grouped[family] = []
            families.append(family)
        grouped[family].append(name)
    return ([name for f in families for name in grouped[f]], grouped, families)


def column_kind(values):
    """`("binary", None)` or `("ramp", (lo, hi))` for a column's values."""
    nums = [v for v in (as_float(x) for x in values) if v is not None]
    if not nums:
        return ("binary", None)
    lo, hi = min(nums), max(nums)
    if all(v in (0.0, 1.0) for v in nums):
        return ("binary", None)
    return ("ramp", (lo, hi))


def ring_palette(grouped, families):
    """A colour per annotation column: a hue per family, shades within it."""
    out = {}
    for (fi, family) in enumerate(families):
        base_hue = (fi / max(len(families), 1)) * 0.85
        cols = grouped[family]
        for (ci, name) in enumerate(cols):
            spread = 0.10 * (ci / max(len(cols) - 1, 1) - 0.5) if len(cols) > 1 else 0.0
            out[name] = hsv_rgb((base_hue + spread) % 1.0, 0.55, 0.72)
    return out


def as_int(x):
    if x is None or x == "":
        return None
    try:
        return int(x)
    except (TypeError, ValueError):
        try:
            return int(float(x))
        except (TypeError, ValueError):
            return None


def as_float(x):
    if x is None or x == "":
        return None
    try:
        return float(x)
    except (TypeError, ValueError):
        return None


def polar(cx, cy, r, a):
    return (cx + r * math.cos(a), cy + r * math.sin(a))


def arc(cx, cy, r, a0, a1):
    if r <= 0.0:
        return [polar(cx, cy, r, a0), polar(cx, cy, r, a1)]
    theta = math.sqrt(8.0 * CURVE_TOL / r)
    steps = max(2, int(abs(a1 - a0) / theta) + 2)
    return [polar(cx, cy, r, a0 + (a1 - a0) * i / (steps - 1)) for i in range(steps)]


def sector(cx, cy, r_in, r_out, a0, a1):
    return arc(cx, cy, r_out, a0, a1) + arc(cx, cy, r_in, a1, a0)


def sectors(cx, cy, r_in, r_out, a0, a1):
    thickness = r_out - r_in
    if thickness <= 0.0 or r_out <= 0.0:
        return []
    limit = 1.0 - max(min(0.25 * thickness / r_out, 1.0), 0.0)
    widest = 2.0 * math.acos(max(-1.0, min(limit, 1.0))) or (math.pi / 6.0)
    sweep = a1 - a0
    parts = max(1, int(math.ceil(abs(sweep) / widest)))
    step = sweep / parts
    return [sector(cx, cy, r_in, r_out, a0 + step * i, a0 + step * (i + 1))
            for i in range(parts)]


class Tree:
    def __init__(self, columns):
        node_ids = [as_int(v) for v in columns[NODE_COL]]
        parents = [as_int(v) for v in columns[PARENT_COL]]
        depths = [as_float(v) or 0.0 for v in columns[DEPTH_COL]]
        is_leaf = [bool(as_int(v)) for v in columns[LEAF_COL]]
        leaf_order = [as_int(v) for v in columns[LEAF_ORDER_COL]]
        self.row_of = {n: i for (i, n) in enumerate(node_ids) if n is not None}
        self.children = {n: [] for n in self.row_of}
        self.parent, self.root = {}, None
        for (i, n) in enumerate(node_ids):
            if n is None:
                continue
            p = parents[i]
            self.parent[n] = p
            if p is None or p not in self.row_of:
                self.root = n
            else:
                self.children[p].append(n)
        self.depth = {n: depths[self.row_of[n]] for n in self.row_of}
        self.is_leaf = {n: is_leaf[self.row_of[n]] for n in self.row_of}
        self.max_depth = max(self.depth.values()) if self.depth else 1.0
        self.tips = sorted(
            (n for n in self.row_of if self.is_leaf[n]),
            key=lambda n: (leaf_order[self.row_of[n]]
                           if leaf_order[self.row_of[n]] is not None else self.row_of[n]))
        self.angle = {}
        step = (2.0 * math.pi - OPEN_ANGLE) / max(len(self.tips) - 1, 1)
        self.step = step
        for (i, t) in enumerate(self.tips):
            self.angle[t] = START_ANGLE + step * i
        self._resolve_angles(self.root)
        self.clade, self.clade_of_tip = {}, {}
        clade_col = columns.get(CLADE_COL)
        if clade_col:
            for t in self.tips:
                self.clade_of_tip[t] = clade_col[self.row_of[t]]
            self._resolve_clades(self.root)

    def _resolve_angles(self, n):
        if n is None:
            return 0.0
        kids = self.children.get(n, [])
        if not kids:
            return self.angle.get(n, 0.0)
        a = [self._resolve_angles(k) for k in kids]
        self.angle[n] = sum(a) / len(a)
        return self.angle[n]

    def _resolve_clades(self, n):
        if n is None:
            return set()
        if self.is_leaf.get(n):
            self.clade[n] = self.clade_of_tip.get(n)
            return {self.clade_of_tip.get(n)}
        seen = set()
        for k in self.children.get(n, []):
            seen |= self._resolve_clades(k)
        self.clade[n] = next(iter(seen)) if len(seen) == 1 else None
        return seen

    def radius_frac(self, n):
        t = self.depth[n] / self.max_depth if self.max_depth > 0 else 0.0
        return R_ROOT + (R_LEAF - R_ROOT) * t

    def clade_runs(self):
        runs, start = [], None
        for t in self.tips:
            c = self.clade_of_tip.get(t)
            if start is None or c != start[0]:
                if start is not None:
                    runs.append(start)
                start = [c, self.angle[t], self.angle[t]]
            else:
                start[2] = self.angle[t]
        if start is not None:
            runs.append(start)
        return [(c, a0, a1) for (c, a0, a1) in runs if c not in (None, "")]


class Tip:
    """A clickable marker for one leaf; its inspector fetches the genome."""

    def __init__(self, key, label, color):
        self.key = key
        self.label = label
        self.color = color
        self._result = None

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else 0.0
        h = base.y.provided_value() if base.y is not None else 0.0
        if w <= 0.0 or h <= 0.0:
            return dex.DrawResult.Complete(region=None)
        cx, cy = base.pos.x + w / 2.0, base.pos.y + h / 2.0
        # A small dot centred in the (larger) click box, so tips read as points,
        # not blobs, even when thousands crowd the ring.
        _polygon(ctx, octagon(cx, cy, min(w, h) * 0.28), self.color)
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))

    def type_name(self):
        return self.label or self.key

    def build_inspector(self, ctx):
        # `build_inspector` gets a NodeContext (`.workspace`), not a DrawContext.
        ws = ctx.workspace.action_handle()
        if self._result is None:
            key = self.key

            def produce(ws):
                ge = build_genome_explorer(ws, fetch_gbff(key))
                ge_uid = ws.insert_node_dyn(ge)
                return framed(ws, ge_uid)

            self._result = async_slot(ws, produce)
        # Owned by this Tip (persistent), borrowed by the inspector via a Ref.
        return ws.insert_node_dyn(Ref(self._result))

    def owned_nodes(self):
        return [self._result] if self._result is not None else []

    def on_delete(self, ctx):
        if self._result is not None:
            ctx.workspace.delete_node(self._result)


class SuperPhylogeny:
    """The circos_table tree — branches, annotation rings, clade rim (all cached
    and run-merged) — plus one inspectable `Tip` node at each *branch tip*, whose
    inspector drills into that leaf's genome."""

    def __init__(self, columns, tips):
        self.columns = {k: list(v) for (k, v) in columns.items()}
        self.tip_nodes = dict(tips)   # tree tip id -> Tip node uid
        names = list(self.columns.keys())
        self.anno_cols, self.grouped, self.families = annotation_columns(names)
        self.kinds = {n: column_kind(self.columns[n]) for n in self.anno_cols}
        self.ring_ink = ring_palette(self.grouped, self.families)
        vals = []
        for v in self.columns.get(CLADE_COL, []):
            if v not in (None, "") and v not in vals:
                vals.append(v)
        self.clade_ink = {v: hsv_rgb((i / max(len(vals), 1)) % 1.0, 0.42, 0.74)
                          for (i, v) in enumerate(vals)}
        self._built = None
        self._paths = []
        self._labels = []
        self._tree = None

    # -- drawing ---------------------------------------------------------

    def draw(self, ctx):
        base = ctx.constraints
        width = base.x.provided_value() if base.x is not None else None
        height = base.y.provided_value() if base.y is not None else None
        if width is None or height is None:
            return dex.DrawResult.Complete(region=None)
        radius = min(width, height) / 2.0 - PADDING
        if radius <= 0.0:
            return dex.DrawResult.Complete(region=None)
        if self._built != (width, height):
            self._build(ctx, radius)
            self._built = (width, height)
        cx = base.pos.x + width / 2.0
        cy = base.pos.y + height / 2.0
        origin = dex.DrawConstraints(pos=dex.ScreenPos.new(cx, cy), x=None, y=None,
                                     wrap=None, should_clip=False)
        for path in self._paths:
            ctx.draw_node(path, origin)
        for (label, ox, oy) in self._labels:
            ctx.draw_node(label, dex.DrawConstraints(
                pos=dex.ScreenPos.new(cx + ox, cy + oy), x=None, y=None,
                wrap=None, should_clip=False))
        # Inspection targets sit at each leaf's *branch endpoint* (its depth
        # radius), inside the annotation rings — a branch tip, not the outer ring.
        tree = self._tree
        if tree is not None:
            for t in tree.tips:
                uid = self.tip_nodes.get(t)
                if uid is None:
                    continue
                (x, y) = polar(cx, cy, radius * tree.radius_frac(t), tree.angle[t])
                ctx.draw_inspectable_node(
                    uid, _box(x - TIP_DOT / 2.0, y - TIP_DOT / 2.0, TIP_DOT, TIP_DOT))
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(width, height)))

    # -- building (once per size) ---------------------------------------

    def _build(self, ctx, radius):
        self._paths, self._labels = [], []
        tree = Tree(self.columns)
        self._tree = tree
        if not tree.tips or tree.root is None:
            return
        self._build_branches(radius, tree)
        self._build_rings(ctx, radius, tree)
        self._build_clades(ctx, radius, tree)

    def _build_branches(self, radius, tree):
        """Branches, tinted by clade; a node whose whole subtree shares one
        colour is one retraced polyline rather than one stroke per branch."""
        weight = max(0.6, BRANCH_WIDTH * radius / BRANCH_WIDTH_AT)
        for n in tree.row_of:
            kids = tree.children.get(n, [])
            if not kids:
                continue
            r = radius * tree.radius_frac(n)
            node_ink = self.clade_ink.get(tree.clade.get(n), BRANCH_INK)
            skids = sorted(kids, key=lambda k: tree.angle[k])
            inks = [self.clade_ink.get(tree.clade.get(k), BRANCH_INK) for k in skids]
            if all(ci == node_ink for ci in inks):
                pts = []
                for (idx, k) in enumerate(skids):
                    a = tree.angle[k]
                    rk = radius * tree.radius_frac(k)
                    if idx > 0:
                        pts.extend(arc(0.0, 0.0, r, tree.angle[skids[idx - 1]], a))
                    else:
                        pts.append(polar(0.0, 0.0, r, a))
                    pts.append(polar(0.0, 0.0, rk, a))
                    pts.append(polar(0.0, 0.0, r, a))
                self._stroke(pts, node_ink, weight)
                continue
            angles = [tree.angle[k] for k in skids]
            self._stroke(arc(0.0, 0.0, r, min(angles), max(angles)), node_ink, weight)
            for (k, k_ink) in zip(skids, inks):
                self._stroke([polar(0.0, 0.0, r, tree.angle[k]),
                              polar(0.0, 0.0, radius * tree.radius_frac(k), tree.angle[k])],
                             k_ink, weight)

    def _ring_runs(self, tree, name):
        """`(a0, a1, rgb)` per drawable run of a ring — merged, absent skipped."""
        kind, span = self.kinds[name]
        ink = self.ring_ink[name]
        col = self.columns[name]
        half = tree.step * (0.5 - BLOCK_INSET)

        def level_colour(v):
            if kind == "binary":
                return ink if v > 0.0 else None
            lo, hi = span
            if hi <= lo:
                return None
            q = round((v - lo) / (hi - lo) * RAMP_LEVELS)
            return lerp_rgb(TRACK_BG, ink, q / RAMP_LEVELS)

        runs, run = [], None
        for t in tree.tips:
            v = as_float(col[tree.row_of[t]])
            rgb = None if v is None else level_colour(v)
            if rgb is None:
                if run is not None:
                    runs.append(run)
                    run = None
                continue
            if run is not None and run[0] == rgb:
                run[2] = tree.angle[t]
            else:
                if run is not None:
                    runs.append(run)
                run = [rgb, tree.angle[t], tree.angle[t]]
        if run is not None:
            runs.append(run)
        return [(a0 - half, a1 + half, rgb) for (rgb, a0, a1) in runs]

    def _build_rings(self, ctx, radius, tree):
        n = len(self.anno_cols)
        if n == 0:
            return
        thickness = (R_ANNO_OUT - R_ANNO_IN) / n
        font = dex.Font.proportional(RING_FONT)
        wrap = dex.TextWrap.singleline()
        a_lo = START_ANGLE - OPEN_ANGLE / 2.0
        a_hi = START_ANGLE + 2.0 * math.pi - 1.5 * OPEN_ANGLE
        for part in sectors(0.0, 0.0, radius * R_ANNO_IN, radius * R_ANNO_OUT, a_lo, a_hi):
            self._polygon(part, TRACK_BG)
        specs = []
        for (i, name) in enumerate(self.anno_cols):
            r_lo = R_ANNO_IN + thickness * i
            r_in = radius * (r_lo + thickness * RING_GAP)
            r_out = radius * (r_lo + thickness * (1.0 - RING_GAP))
            for (b0, b1, rgb) in self._ring_runs(tree, name):
                for part in sectors(0.0, 0.0, r_in, r_out, b0, b1):
                    self._polygon(part, rgb)
            specs.append(((r_in + r_out) / 2.0, name.split(":", 1)[0]))
        if radius >= LABEL_MIN_RADIUS:
            self._place_ring_labels(ctx, specs, font, wrap)

    def _place_ring_labels(self, ctx, specs, font, wrap):
        measured = [(r, text, ctx.measure_text(text, font, wrap)) for (r, text) in specs]
        line_h = max((m.height for (_, _, m) in measured), default=0.0) + 3.0
        prev = None
        for (r, text, m) in measured:
            natural = -r
            cy = natural if prev is None else min(natural, prev - line_h)
            prev = cy
            lbl = dex.Label.new(text)
            lbl.font = font
            lbl.color = dex.Color.rgb(*INK)
            self._labels.append((lbl, -m.width / 2.0, cy - m.height / 2.0))
            if natural - cy > 2.0:
                self._stroke([(0.0, cy + m.height / 2.0), (0.0, natural)],
                             (205, 209, 214), 0.8)

    def _build_clades(self, ctx, radius, tree):
        if not self.clade_ink:
            return
        r_in, r_out = radius * R_CLADE_IN, radius * R_CLADE_OUT
        named = radius >= LABEL_MIN_RADIUS
        font = dex.Font.proportional(CLADE_FONT)
        wrap = dex.TextWrap.singleline()
        pad = tree.step * 0.5
        for (clade, a0, a1) in tree.clade_runs():
            rgb = self.clade_ink.get(clade, BRANCH_INK)
            (b0, b1) = (a0 - pad, a1 + pad)
            for part in sectors(0.0, 0.0, r_in, r_out, b0, b1):
                self._polygon(part, rgb)
            if named and r_out * (b1 - b0) >= MIN_CLADE_LABEL_ARC:
                mid = (b0 + b1) / 2.0
                m = ctx.measure_text(clade, font, wrap)
                (x, y) = polar(0.0, 0.0, r_out + CLADE_LABEL_GAP, mid)
                x += math.cos(mid) * m.width / 2.0
                y += math.sin(mid) * m.height / 2.0
                lbl = dex.Label.new(clade)
                lbl.font = font
                lbl.color = dex.Color.rgb(*INK)
                self._labels.append((lbl, x - m.width / 2.0, y - m.height / 2.0))

    def _stroke(self, pts, rgb, width):
        if len(pts) < 2:
            return
        self._paths.append(dex.Path.polyline(
            [dex.Vector.new(x, y) for (x, y) in pts],
            dex.Stroke.new(width, dex.Color.rgb(*rgb))))

    def _polygon(self, pts, rgb):
        if len(pts) < 3:
            return
        self._paths.append(dex.Path.polygon(
            [dex.Vector.new(x, y) for (x, y) in pts],
            dex.Color.rgb(rgb[0], rgb[1], rgb[2]), dex.Stroke.none()))

    # -- messages --------------------------------------------------------

    def type_name(self):
        return "A Super Phylogeny"

    def owned_nodes(self):
        return list(self.tip_nodes.values())

    def on_delete(self, ctx):
        for uid in self.tip_nodes.values():
            ctx.workspace.delete_node(uid)

    def build_inspector(self, ctx):
        return None


# ======================================================================
# Build and transform
# ======================================================================


def _columns(batch):
    batch = batch.combine_chunks() if hasattr(batch, "combine_chunks") else batch
    return {name: batch.column(i).to_pylist() for (i, name) in enumerate(batch.column_names)}


def build(ws, columns):
    """Build the tree and one inspectable `Tip` node per leaf."""
    tree = Tree(columns)
    key_col = columns.get(KEY_COL, [])
    label_col = columns.get(LABEL_COL, [])
    clade_col = columns.get(CLADE_COL, [])
    inks = {}
    vals = []
    for v in clade_col:
        if v not in (None, "") and v not in vals:
            vals.append(v)
    for (i, v) in enumerate(vals):
        inks[v] = hsv_rgb((i / max(len(vals), 1)) % 1.0, 0.42, 0.7)
    tips = {}
    for t in tree.tips:
        row = tree.row_of[t]
        key = key_col[row] if row < len(key_col) else ""
        if not key:
            continue
        label = label_col[row] if row < len(label_col) else key
        color = inks.get(tree.clade_of_tip.get(t), BRANCH_INK)
        tips[t] = ws.insert_node_dyn(Tip(key, label, color))
    return SuperPhylogeny(columns, tips)


def _find_table():
    for value in globals().values():
        if type(value).__name__ in ("RecordBatch", "Table"):
            return value
    return None


def transform():
    batch = _find_table()
    if batch is None:
        raise ValueError("wire the phylogeny Table into this transform")
    return build(dex.ws, _columns(batch))
