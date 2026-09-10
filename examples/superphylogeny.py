"""One taxonomic tree that drills all the way down to structure.

Three views, joined into a single object you explore by drilling inward:

  1. **The tree** — a circular phylogeny from the GTDB-style table (branches
     tinted by phylum, a labelled clade ring, a dot at each tip).
  2. **Click a tip → its genome.** Hovering a tip names its leaf; clicking one
     opens a small panel in the tree's foreground with an "Open genome" button.
     That fetches the assembly from NCBI on a background task — "Loading…" shown
     fullscreen while it is in flight — then the Genome Explorer of that genome
     fills the screen.
  3. **Click a gene → its structure.** Hovering a gene names it; clicking one
     opens a panel with its record and, for a CDS, an "Open structure" button
     that fetches the AlphaFold model (or folds the sequence with ESMFold) and
     opens it fullscreen.

**One sensor, not a node per point.** The tree has thousands of tips and a genome
thousands of genes; a clickable *node* at each — drawn and hit-tested every
frame — is what made a full tree crawl. Instead each plot draws its own marks,
records where they landed, and answers hover and click by searching that list,
the way `pdb_viewer.py` picks an atom. The hover readout and the click panel live
in the plane's *foreground*, reading the selection off the plot object directly —
the `Protein`/`AtomPanel` split — so they stay put and legible at any zoom.

Every one of these views is bigger than the box it is shown in, so each sits on a
`Canvas` of its own (`on_plane`): drawn once, life-size, and moved by dragging
and alt-scrolling. The structure is the exception, giving up the plane's drag in
exchange for being turned by it. Each plane is named after what it holds, so the
breadcrumb trail says "Genome" and not "A Canvas".

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
if you want them.
"""

import io
import json
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
ALPHAFOLD_API = "https://alphafold.ebi.ac.uk/api/prediction/{uni}"
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
    """The AlphaFold predicted structure (PDB) for a UniProt accession.

    The model URL is *asked for*, not composed. The file names carry a database
    version (`…-model_v6.pdb`), and it moves: every hard-coded `v4` URL is now a
    404, which — being caught as "no model" — read as "AlphaFold has nothing for
    this protein" for every protein in the archive. The prediction endpoint
    answers with the current URL, so this cannot go stale again.
    """
    req = urllib.request.Request(ALPHAFOLD_API.format(uni=uniprot), headers=UA)
    entries = json.loads(urllib.request.urlopen(req, timeout=TIMEOUT).read())
    url = next((e.get("pdbUrl") for e in entries if e.get("pdbUrl")), None)
    if not url:
        raise urllib.error.HTTPError(req.full_url, 404, "no AlphaFold model", None, None)
    return urllib.request.urlopen(
        urllib.request.Request(url, headers=UA), timeout=TIMEOUT
    ).read().decode("utf-8", "replace")


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


def _uniprot_first(query):
    """The first UniProt accession a `query` returns, or "" — one request."""
    url = UNIPROT_SEARCH.format(q=urllib.parse.quote(query))
    body = urllib.request.urlopen(
        urllib.request.Request(url, headers=UA), timeout=TIMEOUT
    ).read().decode("utf-8", "replace").strip()
    return body.splitlines()[0].strip() if body else ""


def resolve_uniprot(protein_id):
    """A UniProt accession for a RefSeq/GenBank `protein_id`, via UniProtKB.

    The AlphaFold archive is keyed by UniProt, but a CDS usually carries only a
    `/protein_id` (WP_…/NP_…). This maps one to the other so any annotated CDS —
    not just those with a UniProtKB `/db_xref` — can reach a model. A miss is
    cached as "" too, so it is not retried.

    Asked more than one way, most specific first, because `xref:` alone is *not*
    how UniProtKB indexes a RefSeq accession — it wants the database qualified,
    `xref:refseq-WP_…`, and the bare form quietly matches nothing, which is why
    this looked like AlphaFold having no model for anything. The qualified query
    is tried first, then the bare `xref:`, then the accession as a plain term;
    the first that answers wins.
    """
    if not protein_id:
        return ""
    if protein_id in _UNIPROT_CACHE:
        return _UNIPROT_CACHE[protein_id]
    bare = protein_id.split(".")[0]  # UniProt xrefs are unversioned
    acc = ""
    for query in ("xref:refseq-" + bare, "xref:" + bare, bare):
        try:
            acc = _uniprot_first(query)
        except Exception:  # noqa: BLE001 - a lookup failure is just a miss
            acc = ""
        if acc:
            break
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
# Shared constants and draw helpers
# ======================================================================

# A structure or genome shown in an inspector preview can be handed a non-finite
# axis on a measure pass; these are the fallbacks it falls back to.
THUMB_W = 380.0
THUMB_H = 280.0
INK = (58, 62, 70)
FAINT = (120, 126, 136)


# `on_plane` is the prelude's: a body on a pan/zoom canvas of its own, with the
# things that stay put while the plane moves under it handed in as `foreground`.
# This file used to carry its own copy, which — sharing the prelude's namespace —
# quietly shadowed it; the one place a view here differs (a structure gives up
# the plane's drag to be turned by it) is expressed in what its sensor senses,
# not in a second `on_plane`.


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


def bohr_shells(z):
    """One ring per Bohr shell for atomic number `z`: radius, electrons, plane.

    A shell model is all the electron structure the periodic table hands you
    without orbitals, so it is drawn as one: rings you can count, with the
    electrons on them as discrete points. The rings do not lie in one plane —
    each is tilted a little further than the last — so the model reads as a
    solid object turning rather than as flat concentric circles.
    """
    counts = electron_shells(z) or [0]
    n = len(counts)
    return [((i + 1) / n, count, 0.30 + i * (0.9 / max(n, 1)))
            for (i, count) in enumerate(counts)]


def _turn(p, yaw, tilt):
    """A point through the viewer's yaw and tilt, the way the protein turns."""
    (x, y, z) = p
    (ca, sa) = (math.cos(yaw), math.sin(yaw))
    (cb, sb) = (math.cos(tilt), math.sin(tilt))
    (x, z) = (x * ca + z * sa, -x * sa + z * ca)
    (y, z) = (y * cb - z * sb, y * sb + z * cb)
    return (x, y, z)


def _on_ring(r, plane, angle):
    """The point at `angle` around a ring of radius `r`, in its own tilted plane."""
    (ct, st) = (math.cos(plane), math.sin(plane))
    (x, y) = (r * math.cos(angle), r * math.sin(angle))
    return (x, y * ct, y * st)


def shell_color(i, n):
    """Inner shells blue, outer shells warm — near the nucleus to far from it."""
    return lerp_rgb((70, 110, 198), (206, 132, 96), i / max(n - 1, 1))


def draw_bohr(ctx, cx, cy, R, shells, yaw, tilt, dot=3.2, arcs=72):
    """The model, depth-sorted about its nucleus.

    Everything — every arc of every ring, every electron, the nucleus itself —
    goes into one list keyed by depth and is painted back to front, so the near
    half of a ring passes in front of the nucleus and the far half behind it.
    That, and fading with depth, is the whole of the three-dimensionality.
    """
    n = len(shells)
    nucleus_r = max(3.0, R * 0.09)
    items = [(0.0, "nucleus", None)]
    for (i, (r, count, plane)) in enumerate(shells):
        ink = shell_color(i, n)
        ring = [_turn(_on_ring(r, plane, 2 * math.pi * k / arcs), yaw, tilt)
                for k in range(arcs)]
        for k in range(arcs):
            (a, b) = (ring[k], ring[(k + 1) % arcs])
            items.append(((a[2] + b[2]) / 2.0, "arc", (a, b, ink)))
        for e in range(count):
            q = _turn(_on_ring(r, plane, 2 * math.pi * e / count), yaw, tilt)
            items.append((q[2], "electron", (q, ink)))

    items.sort(key=lambda it: it[0])
    lo = items[0][0]
    span = (items[-1][0] - lo) or 1.0
    for (depth, kind, payload) in items:
        t = (depth - lo) / span
        if kind == "nucleus":
            _polygon(ctx, circle_pts(cx, cy, nucleus_r, 28), (86, 92, 102))
        elif kind == "arc":
            (a, b, ink) = payload
            shade = lerp_rgb(lerp_rgb(ink, FAR, 0.78), ink, t)
            _line(ctx, [(cx + a[0] * R, cy - a[1] * R), (cx + b[0] * R, cy - b[1] * R)],
                  shade, 1.0 + 0.5 * t)
        else:
            (q, ink) = payload
            shade = lerp_rgb(lerp_rgb(ink, FAR, 0.42), ink, t)
            _polygon(ctx, octagon(cx + q[0] * R, cy - q[1] * R, dot * (0.66 + 0.6 * t)), shade)


def shell_counts(shells):
    """`2 \u00b7 8 \u00b7 18`: the model's own caption."""
    return " \u00b7 ".join(str(count) for (_r, count, _plane) in shells)


class AtomModel:
    """A single atom as a rotatable 3D Bohr model — driven by the same
    drag-to-rotate machinery as the protein, and pushable fullscreen."""

    def __init__(self, element, z, sensor):
        self.element = element
        self.z = z
        self.sensor = sensor
        self.shells = bohr_shells(z)
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
        draw_bohr(ctx, cx, cy, R, self.shells, self.yaw, self.tilt,
                  dot=max(3.0, R * 0.022))
        _text(ctx, "%s  ·  %s  ·  Z %d"
              % (ELEMENT_NAME.get(self.element, self.element), self.element, self.z),
              base.pos.x + 10.0, base.pos.y + 8.0, 12.0, INK)
        # What the rings are: the electron count per shell, outward.
        _text(ctx, "shells  %s" % shell_counts(self.shells),
              base.pos.x + 10.0, base.pos.y + 26.0, 10.0, FAINT)
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))

    def type_name(self):
        return "%s atom" % (ELEMENT_NAME.get(self.element, self.element))

    def owned_nodes(self):
        return [self.sensor] + list(self.kept.values())

    def on_delete(self, ctx):
        for uid in self.owned_nodes():
            ctx.workspace.delete_node(uid)

    def build_inspector(self, ctx):
        return None


class Protein:
    """Every atom as a depth-sorted CPK dot over the backbone trace.

    Drag it to turn it — the whole surface, so there is nothing to aim at first.
    It sits on a plane, so alt-scroll magnifies it; the plane's own drag is given
    up in exchange, which is the trade a thing you turn has to make.

    Click an atom to select it: it is ringed, and the `AtomPanel` pinned in the
    plane's foreground shows what it is.
    """

    def __init__(self, title, order, chains, atoms, sensor):
        self.title = title
        self.order = list(order)
        self.chains = {c: list(p) for (c, p) in chains.items()}
        self.atoms = list(atoms)   # (element, chain, resname, resseq, name, (x,y,z))
        self.sensor = sensor
        self.yaw = 0.0
        self.tilt = PROT_TILT
        # Read by the `AtomPanel` in the foreground: an index into `atoms`, or
        # None. The structure is the only thing that can know which atom a click
        # landed on, so it is the one that says.
        self.selected = None
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
            self.yaw += drag.x * DRAG_SENS
            self.tilt = max(-1.4, min(1.4, self.tilt + drag.y * DRAG_SENS))

        avail = min(w, h) / 2.0 - 12.0
        if avail <= 0.0 or not self.atoms:
            return dex.DrawResult.Complete(
                region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))
        scale = avail / self.radius
        ox = base.pos.x + w / 2.0
        oy = base.pos.y + h / 2.0
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

        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))

    def type_name(self):
        return "A PDB Viewer"

    def owned_nodes(self):
        return [self.sensor]

    def on_delete(self, ctx):
        ctx.workspace.delete_node(self.sensor)

    def build_inspector(self, ctx):
        return None


class AtomPanel:
    """What the selected atom is, pinned to the plane's top-left.

    A foreground, so it stays put and life-size: drawn with the structure it
    would turn with the model and grow with the magnification, which is the
    opposite of what a readout is for.

    It reads the selection off the `Protein` object itself rather than through a
    message. The two are one view drawn in two bands — only the structure can
    know which atom a click landed on — and a plain Python reference is the
    honest way to say so. (A deep copy of the surface splits them; the price of
    not inventing a message for one field.)
    """

    def __init__(self, protein, model_button):
        self.protein = protein
        self.model_button = model_button
        self.atom_model_uid = None
        self._model_element = None
        self._shell_elem = None     # the cached still, keyed by element
        self._shells = None

    def draw(self, ctx):
        base = ctx.constraints
        picked = self.protein.selected
        if picked is None or picked >= len(self.protein.atoms):
            return dex.DrawResult.Complete(region=None)

        ws = ctx.node.workspace
        (x, y) = (base.pos.x + PANEL_INSET, base.pos.y + PANEL_INSET)
        (elem, ch, resname, resseq, name, coord) = self.protein.atoms[picked]
        z = ELEMENT_Z.get(elem, 0)
        (pw, ph) = PANEL_SIZE
        ctx.draw_node(
            dex.Rect.bordered(pw, ph, dex.Color.rgba(255, 255, 255, 242), 6.0,
                              dex.Stroke.new(1.0, dex.Color.rgb(*SCRUB_EDGE))),
            _box(x, y, pw, ph),
        )
        _text(ctx, "%s %s%s" % (resname or "?", ch.strip() or "?", resseq),
              x + 8.0, y + 6.0, 11.0, INK)
        _text(ctx, "atom %s" % name, x + 8.0, y + 22.0, 10.0, (90, 96, 104))
        _text(ctx, "%s (%s, Z %d)" % (ELEMENT_NAME.get(elem, elem), elem, z),
              x + 8.0, y + 36.0, 10.0, (90, 96, 104))
        _text(ctx, "x %.2f  y %.2f  z %.2f" % coord, x + 8.0, y + 50.0, 9.0, FAINT)
        # A still of the Bohr model; the fullscreen one turns.
        if elem != self._shell_elem:
            self._shells = bohr_shells(z)
            self._shell_elem = elem
        draw_bohr(ctx, x + pw / 2.0, y + 94.0, 30.0, self._shells, 0.6, 0.32, dot=2.0)
        _text(ctx, shell_counts(self._shells), x + 8.0, y + ph - 44.0, 9.0, FAINT)
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
        return dex.DrawResult.Complete(region=None)

    def type_name(self):
        return "Atom Readout"

    def owned_nodes(self):
        out = [self.model_button]
        if self.atom_model_uid is not None:
            out.append(self.atom_model_uid)
        return out

    def on_delete(self, ctx):
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


# The structure is drawn once at this size and then magnified from there.
PROT_SIZE = 900.0
# The atom readout, pinned in the plane's foreground.
PANEL_SIZE = (176.0, 168.0)
PANEL_INSET = 12.0
SCRUB_EDGE = (206, 210, 218)


def build_protein(ws, pdb_text):
    """The structure on a plane of its own, with its readout pinned in front.
    Returns the canvas, which is what goes in a slot or fullscreen."""
    (title, order, chains, atoms) = parse_pdb(pdb_text)
    # Clicks pick an atom, drags turn the structure.
    sensor = ws.insert_node_dyn(dex.InteractionBox.sensing(False, True, True))
    protein = Protein(title, order, chains, atoms, sensor)
    body = ws.insert_node_dyn(protein)
    model_button = dex.Button.build(ws, dex.Label.new("Open Fullscreen"))
    panel = ws.insert_node_dyn(AtomPanel(protein, model_button))
    return on_plane(ws, body, (PROT_SIZE, PROT_SIZE), name="Structure",
                    foreground=[panel], what="Placed the structure")


# ======================================================================
# Genome explorer + genes (see genome_explorer.py; genes fetch structures)
# ======================================================================

FEATURE_COLORS = {
    "CDS": (86, 124, 176), "tRNA": (94, 168, 116), "rRNA": (206, 132, 84),
    "ncRNA": (150, 110, 180), "tmRNA": (176, 120, 168), "regulatory": (200, 176, 90),
}
G_MARGIN, G_TITLE, ROW_H, BAND, LINE_HALF = 12.0, 22.0, 40.0, 8.0, 5.0
# The map is laid out once at this size — rows enough for a whole genome at a
# readable scale — and then panned and zoomed, rather than reflowed to whatever
# box it is being shown in.
GENOME_SIZE = (1500.0, 1150.0)
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


class GenomeExplorer:
    """The wrapped genome map, its genes picked by one sensor rather than a node
    apiece — thousands of inspectable gene-nodes were what made this slow.

    It draws every gene arrow itself, records each one's box, and hit-tests the
    pointer against them; the `GenePanel` in the plane's foreground reads the
    hovered and selected gene off this object and shows them. What the map is and
    what its colours mean are `GenomeChrome`, also in the foreground, where they
    stay legible however far the plane has been dragged or magnified.
    """

    def __init__(self, records, offsets, genes, total_len, types_present, sensor):
        self.records = list(records)
        self.offsets = list(offsets)
        self.genes = list(genes)   # gene dicts: layout span + record + colour
        self.total_len = total_len
        self.types_present = list(types_present)
        self.sensor = sensor
        self.hover = None          # gene index under the pointer, or None
        self.selected = None       # gene index a click chose, or None
        self._boxes = []           # [(x0, y0, x1, y1, index)] this frame
        # Structures folded for this genome's genes, by gene index. Kept here
        # rather than on the readout that asks for them: a panel is chrome and
        # gets rebuilt, and a structure that took a fetch and a fold to get
        # should not go with it. This is what a clone copies and a delete takes.
        self.kept = {}
        defn = (records[0][2] if records else "") or (records[0][0] if records else "genome")
        self.title = "%s — %d bp · %d features" % (defn, total_len, len(genes))

    def __getstate__(self):
        state = self.__dict__.copy()
        state["_boxes"] = []
        return state

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
        self._boxes = []
        x0, y0 = base.pos.x + G_MARGIN, base.pos.y + G_MARGIN
        plot_w, plot_h = width - 2 * G_MARGIN, height - 2 * G_MARGIN
        # The title and the key sit in the plane's foreground (`GenomeChrome`),
        # so the map only leaves room for them.
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
        for (gi, g) in enumerate(self.genes):
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
            _polygon(ctx, arrow_points(gx0, gx1, gy0, gy1, g["strand"]), g["color"])
            self._boxes.append((gx0, gy0, gx1, gy1, gi))

        # One sensor over the whole map: hover to read a gene, click to select.
        ws = ctx.node.workspace
        _clip = dex.DrawConstraints(
            pos=base.pos, x=dex.AxisConstraint.Exactly(width),
            y=dex.AxisConstraint.Exactly(height), wrap=None, should_clip=False)
        ctx.draw_node(self.sensor, _clip)
        pointer = ws.send_request(self.sensor, dex.PointerPos())
        self.hover = self._gene_at(pointer) if pointer is not None else None
        if ws.send_request(self.sensor, dex.TakeClicked()) and pointer is not None:
            self.selected = self.hover
        return self._done(base, width, height)

    def _gene_at(self, p):
        # Last drawn first, so a gene on a later row wins where rows are dense.
        for (x0, y0, x1, y1, gi) in reversed(self._boxes):
            if x0 <= p.x <= x1 and y0 <= p.y <= y1:
                return gi
        return None

    def _done(self, base, w, h):
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))

    def type_name(self):
        return "A Genome Explorer"

    def owned_nodes(self):
        return [self.sensor]

    def on_delete(self, ctx):
        ctx.workspace.delete_node(self.sensor)

    def build_inspector(self, ctx):
        return None


class GenePanel:
    """The hovered and selected gene, pinned in the genome plane's foreground.

    It reads the selection off the `GenomeExplorer` object directly — the two are
    one view in two bands, and only the map can know which gene a click landed on.
    A hovered gene gets a one-line readout along the bottom; a clicked CDS gets a
    card with its record and an "Open structure" button, which fetches the model
    (AlphaFold, else ESMFold from the sequence) and opens it fullscreen.
    """

    def __init__(self, explorer, button):
        self.explorer = explorer
        self.button = button

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        h = base.y.provided_value() if base.y is not None else None
        if w is None or h is None or not (math.isfinite(w) and math.isfinite(h)):
            return dex.DrawResult.Complete(region=None)
        ge = self.explorer
        genes = ge.genes

        if ge.hover is not None and ge.hover < len(genes):
            g = genes[ge.hover]
            name = g["name"] or g["locus"] or g["key"]
            line = "%s (%s)" % (name, g["key"])
            if g["product"]:
                line += " — " + g["product"]
            _text(ctx, line, base.pos.x + 12.0, base.pos.y + h - 22.0, 11.0, INK)

        if ge.selected is not None and ge.selected < len(genes):
            self._overlay(ctx, base, w, h, ge.selected)
        return dex.DrawResult.Complete(region=None)

    def _overlay(self, ctx, base, w, h, gi):
        g = self.explorer.genes[gi]
        rows = []
        if g["name"]:
            rows.append("Gene: %s" % g["name"])
        if g["locus"]:
            rows.append("Locus tag: %s" % g["locus"])
        rows.append("Type: %s" % g["key"])
        if g["product"]:
            rows.append("Product: %s" % g["product"])
        if g["uniprot"]:
            rows.append("UniProt: %s" % g["uniprot"])
        arrow = "→" if g["strand"] >= 0 else "←"
        rows.append("Location: %d %s %d" % (g["loc_start"], arrow, g["loc_end"]))

        pw = 230.0
        font_h = 15.0
        can_fold = g["key"] == "CDS" and (g["uniprot"] or g["protein_id"] or g["translation"])
        ph = len(rows) * font_h + 16.0 + (30.0 if can_fold else 0.0)
        x = base.pos.x + w - pw - 12.0
        y = base.pos.y + 12.0
        ctx.draw_node(
            dex.Rect.bordered(pw, ph, dex.Color.rgba(255, 255, 255, 244), 5.0,
                              dex.Stroke.new(1.0, dex.Color.rgb(*SCRUB_EDGE))),
            _box(x, y, pw, ph))
        for (k, s) in enumerate(rows):
            _text(ctx, s, x + 9.0, y + 8.0 + k * font_h, 10.5, INK)
        if can_fold:
            by = y + ph - 26.0
            ctx.draw_node(self.button, _box(x + 9.0, by, pw - 18.0, 20.0))
            if ctx.node.workspace.send_request(self.button, dex.TakeClicked()):
                self._open_structure(ctx.node.workspace.action_handle(), gi)

    def _open_structure(self, ws, gi):
        """Fetch (once) and push the gene's structure fullscreen.

        Kept by the explorer, not by this panel: see `GenomeExplorer.kept`.
        """
        if gi in self.explorer.kept:
            ws.submit_action(ws.root(), dex.PushOverride(node=self.explorer.kept[gi]),
                             "Structure fullscreen")
            return
        g = self.explorer.genes[gi]
        uni, pid, seq = g["uniprot"], g["protein_id"], g["translation"]

        def produce(ws):
            acc = uni or resolve_uniprot(pid)
            if acc:
                try:
                    return build_protein(ws, fetch_alphafold(acc))
                except urllib.error.HTTPError as e:
                    # 404: no model for it; 400: not an accession AlphaFold
                    # indexes. Either way, fall through to ESMFold.
                    if e.code not in (400, 404):
                        raise
            if seq and len(seq) <= ESMFOLD_MAX:
                return build_protein(ws, fetch_esmfold(seq))
            if seq:
                return _note("No AlphaFold model; %d aa is over the ESMFold "
                             "limit (%d)." % (len(seq), ESMFOLD_MAX))
            return _note("No structure available for this gene.")

        uid = async_slot(ws, produce)
        self.explorer.kept[gi] = uid
        ws.submit_action(ws.root(), dex.PushOverride(node=uid), "Structure fullscreen")

    def type_name(self):
        return "Gene Readout"

    def owned_nodes(self):
        return [self.button]

    def on_delete(self, ctx):
        for uid in self.owned_nodes():
            ctx.workspace.delete_node(uid)

    def build_inspector(self, ctx):
        return None


class GenomeChrome:
    """What the map is, and what its colours mean — pinned to the plane.

    A foreground: the title and the key are the two things you must be able to
    read at any magnification and wherever the plane has been dragged to, and
    drawn with the map they were the first things to leave the screen.
    """

    def __init__(self, title, types_present):
        self.title = str(title)
        self.types_present = list(types_present)

    def draw(self, ctx):
        base = ctx.constraints
        if base.x is None or base.y is None:
            return dex.DrawResult.Complete(region=None)
        (width, height) = (base.x.provided_value(), base.y.provided_value())
        if not (math.isfinite(width) and math.isfinite(height)):
            return dex.DrawResult.Complete(region=None)
        (x0, y0) = (base.pos.x + G_MARGIN, base.pos.y + G_MARGIN)
        plot_w = width - 2 * G_MARGIN

        font = dex.Font.proportional(12.0)
        wrap = dex.TextWrap.singleline()
        # Truncate the title to the panel width, so it never spills the box.
        title = self.title
        tm = ctx.measure_text(title, font, wrap)
        if tm.width > plot_w and len(title) > 4:
            avg = tm.width / len(title)
            keep = max(4, int(plot_w / max(avg, 1.0)) - 1)
            title = title[:keep].rstrip() + "\u2026"
        _text(ctx, title, x0, y0, 12.0, INK)

        # A swatch and name per feature type present, left to right.
        (x, y) = (x0, base.pos.y + height - G_MARGIN - G_LEGEND + 4.0)
        key_font = dex.Font.proportional(10.0)
        sw = 11.0
        right = x0 + plot_w
        for key in self.types_present:
            m = ctx.measure_text(key, key_font, wrap)
            if x + sw + 4.0 + m.width > right:
                break
            _polygon(ctx, [(x, y), (x + sw, y), (x + sw, y + sw), (x, y + sw)],
                     FEATURE_COLORS.get(key, (150, 150, 156)))
            _text(ctx, key, x + sw + 4.0, y + (sw - m.height) / 2.0, 10.0, INK)
            x += sw + 6.0 + m.width + 16.0
        return dex.DrawResult.Complete(region=None)

    def type_name(self):
        return "Genome Key"

    def build_inspector(self, ctx):
        return None


def build_genome_explorer(ws, gbff_text):
    """Parse `gbff_text` into a `GenomeExplorer` — genes as plain data, one sensor
    for the lot, no node per gene."""
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
            tr = quals.get("translation", "")
            genes.append({
                "start": offset + s, "end": offset + e, "loc_start": s, "loc_end": e,
                "strand": strand, "key": key, "name": quals.get("gene", ""),
                "locus": quals.get("locus_tag", ""), "product": quals.get("product", ""),
                "uniprot": uniprot_of(quals), "protein_id": quals.get("protein_id", ""),
                "translation": tr if isinstance(tr, str) else "",
                "color": FEATURE_COLORS.get(key, (150, 150, 156)),
            })
            if key not in types_present:
                types_present.append(key)
        offset += length
    sensor = ws.insert_node_dyn(dex.InteractionBox.sensing(True, True, False))
    return GenomeExplorer(recmeta, offsets, genes, offset, types_present, sensor)


def build_genome_plane(ws, gbff_text):
    """A genome on a pan/zoom plane, its key and gene panel in the foreground.

    The `GenePanel` shares the `GenomeExplorer` object (not a copy), so the click
    the map records is the click the panel reads — the `Protein`/`AtomPanel` split
    exactly.
    """
    explorer = build_genome_explorer(ws, gbff_text)
    body = ws.insert_node_dyn(explorer)
    chrome = ws.insert_node_dyn(GenomeChrome(explorer.title, explorer.types_present))
    button = dex.Button.build(ws, dex.Label.new("Open structure"))
    panel = ws.insert_node_dyn(GenePanel(explorer, button))
    return on_plane(ws, body, GENOME_SIZE, name="Genome",
                    foreground=[chrome, panel], what="Placed the genome")


# ======================================================================
# The tree
# ======================================================================
#
# The tree itself is the prelude's `Phylogeny`, in edge-list mode: one row per
# node, a `parent` column giving the edges, `depth` carrying cumulative branch
# length so the radii are to scale, and `leaf_order` fixing where the tips fall.
# So none of the geometry lives here any more — and, more to the point, this
# tree answers the same protocol every other view does, which is what lets a
# genome or a structure opened from it be joined back to the row it came from.
#
# What is left is the drilling: a panel in the plane's foreground that reads the
# selection off the tree and offers to fetch what that leaf points at.

NODE_COL, PARENT_COL, DEPTH_COL = "node", "parent", "depth"
LEAF_COL, LEAF_ORDER_COL, KEY_COL = "is_leaf", "leaf_order", "key"
CLADE_COL, LABEL_COL = "phylum", "label"

#: Columns that describe the tree rather than annotate it.
STRUCTURAL = {NODE_COL, PARENT_COL, DEPTH_COL, LEAF_COL, LEAF_ORDER_COL,
              LABEL_COL, CLADE_COL, KEY_COL, "distance", "genome", "structures",
              "kind", "n_contigs", "domain"}

#: How thick an annotation ring is, and the gap before the first.
RING_W = 14.0
RING_GAP = 10.0
#: How many rings are drawn before the rest are dropped: past this they are too
#: thin to read, and a ring that cannot be read is worse than no ring.
MAX_RINGS = 6

TIP_DOT = 6.0
SCRUB_EDGE = (208, 213, 222)


def annotation_columns(names):
    """The columns worth drawing as rings, grouped and ordered by family.

    An annotation is named `thing:family` — `zinc:metal`, `glycolysis:process` —
    so the families are read off the names and each gets its own hue, with
    shades within it. Which is what makes twenty rings legible: you are looking
    for a band of colour, not a particular one of twenty.
    """
    chosen = [n for n in names if ":" in n and n not in STRUCTURAL]
    families, grouped = [], {}
    for name in chosen:
        family = name.split(":", 1)[1]
        if family not in grouped:
            grouped[family] = []
            families.append(family)
        grouped[family].append(name)
    return ([name for f in families for name in grouped[f]], grouped, families)


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


def as_float(x):
    if x is None or x == "":
        return None
    try:
        return float(x)
    except (TypeError, ValueError):
        return None


class AnnotationRings:
    """Rings of per-tip annotation outside the tree.

    Placed by asking the tree where it drew each row, rather than by working the
    angles out again — so the rings follow whatever the tree did, at whatever
    zoom, and cannot end up pointing at the wrong tip.
    """

    def __init__(self, tree, columns, inks):
        self.tree = tree
        self.columns = list(columns)
        self.inks = dict(inks)

    def owned_nodes(self):
        return []

    def type_name(self):
        return "Annotation Rings"

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        h = base.y.provided_value() if base.y is not None else None
        if w is None or h is None or not self.columns:
            return dex.DrawResult.Complete(region=None)
        ws = ctx.node.workspace
        drawn = ws.send_request(self.tree, DrawnPoints()) or {}
        table = ws.send_request(self.tree, SourceTable())
        if not drawn or table is None:
            return dex.DrawResult.Complete(region=None)

        # Only the tips carry annotations; an internal node has no value to show.
        order = (table.column(LEAF_ORDER_COL).to_pylist()
                 if LEAF_ORDER_COL in table.column_names else None)
        tips = {row: to_local(ctx, p) for (row, p) in drawn.items()
                if order is None or (row < len(order) and is_num(order[row]))}
        if not tips:
            return dex.DrawResult.Complete(region=None)

        points = list(tips.values())
        cx = sum(p[0] for p in points) / len(points)
        cy = sum(p[1] for p in points) / len(points)
        reach = max(math.hypot(p[0] - cx, p[1] - cy) for p in points)
        if reach <= 1.0:
            return dex.DrawResult.Complete(region=None)
        width = 2.0 * math.pi / max(len(tips), 1)

        for (band, column) in enumerate(self.columns[:MAX_RINGS]):
            r0 = reach + RING_GAP + band * RING_W
            values = table.column(column).to_pylist()
            ink = self.inks.get(column, BRANCH_INK)
            for (row, (px, py)) in tips.items():
                if row >= len(values):
                    continue
                v = as_float(values[row])
                if not v:
                    continue
                angle = math.atan2(py - cy, px - cx)
                polygon(ctx, sector_points(cx, cy, r0, r0 + RING_W - 3.0,
                                           angle - width / 2.0, angle + width / 2.0),
                        ink)
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))


class TreePanel:
    """The hovered and selected leaf, pinned in the tree plane's foreground.

    It knows the tree only by id and asks it the protocol's own questions — the
    hovered row, the selected one, that row's record. So the drilling is written
    against the same messages any other consumer uses, and would work just as
    well against a scatter of the same table.

    A hover names the leaf along the bottom; a click shows its label and an
    "Open genome" button, which fetches that leaf's genome and opens it
    fullscreen.
    """

    def __init__(self, tree, button):
        self.tree = tree
        self.button = button

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        h = base.y.provided_value() if base.y is not None else None
        if w is None or h is None or not (math.isfinite(w) and math.isfinite(h)):
            return dex.DrawResult.Complete(region=None)
        ws = ctx.node.workspace

        hovered = ws.send_request(self.tree, HoverRow())
        if hovered is not None:
            record = ws.send_request(self.tree, RowValues(hovered)) or {}
            _text(ctx, "%s   ·   %s" % (record.get(LABEL_COL, ""), record.get(KEY_COL, "")),
                  base.pos.x + 12.0, base.pos.y + h - 22.0, 11.0, INK)

        selected = ws.send_request(self.tree, Selection())
        if selected is not None:
            record = ws.send_request(self.tree, RowValues(selected)) or {}
            if record.get(KEY_COL):
                self._overlay(ctx, ws, base, w, h, selected, record)
        return dex.DrawResult.Complete(region=None)

    def _overlay(self, ctx, ws, base, w, h, row, record):
        (key, label) = (record.get(KEY_COL, ""), record.get(LABEL_COL, ""))
        font = dex.Font.proportional(11.0)
        wrap = dex.TextWrap.singleline()
        rows = [label, key]
        pw = max(180.0, max(ctx.measure_text(s, font, wrap).width for s in rows) + 18.0)
        ph = len(rows) * 15.0 + 16.0 + 30.0
        x = base.pos.x + w - pw - 12.0
        y = base.pos.y + 12.0
        ctx.draw_node(
            dex.Rect.bordered(pw, ph, dex.Color.rgba(255, 255, 255, 244), 5.0,
                              dex.Stroke.new(1.0, dex.Color.rgb(*SCRUB_EDGE))),
            _box(x, y, pw, ph))
        _text(ctx, label, x + 9.0, y + 8.0, 12.0, INK)
        _text(ctx, key, x + 9.0, y + 24.0, 10.0, FAINT)
        ctx.draw_node(self.button, _box(x + 9.0, y + ph - 26.0, pw - 18.0, 20.0))
        if ws.send_request(self.button, dex.TakeClicked()):
            self._open(ws, row, key)

    def _open(self, ws, row, key):
        """Show this leaf's genome, fetching it the first time only.

        The genome is kept by the *tree*, not by this panel. A readout is chrome
        — redrawn, replaced, rebuilt — and a genome that took a network round
        trip to get should not go with it. The tree is what a clone copies and
        what a delete cleans up after, so that is where it belongs.
        """
        kept = ws.send_request(self.tree, KeptNode(row))
        if kept is not None:
            ws.action_handle().submit_action(
                ws.action_handle().root(), dex.PushOverride(node=kept),
                "Genome fullscreen")
            return

        def produce(handle):
            return build_genome_plane(handle, fetch_gbff(key))

        handle = ws.action_handle()
        uid = ws.send_request(self.tree, KeepNode(row, async_slot(handle, produce)))
        handle.submit_action(handle.root(), dex.PushOverride(node=uid),
                             "Genome fullscreen")

    def type_name(self):
        return "Leaf Readout"

    def owned_nodes(self):
        return [self.button]

    def on_delete(self, ctx):
        for uid in self.owned_nodes():
            ctx.workspace.delete_node(uid)

    def build_inspector(self, ctx):
        return None


# ======================================================================
# The canvas the tree sits on
# ======================================================================

# The tree is drawn once at this size and then panned and zoomed as a whole, so
# it is generous: room for the labels to breathe when the plane is magnified.
TREE_SIZE = 1100.0


# ======================================================================
# Build and transform
# ======================================================================


def build(ws, source):
    """The tree, its annotation rings, and the leaf panel in its foreground."""
    frame = Frame(source)
    tree = build_plot(
        ws, Phylogeny,
        frame=frame,
        x=NODE_COL,
        parent=PARENT_COL,
        depth=DEPTH_COL,
        leaf_order=LEAF_ORDER_COL,
        color=CLADE_COL,
        key=KEY_COL,
        shape="circular",
    )
    # The plane's foreground shows the readout, so the tree does not draw its
    # own: a card drawn inside the tree would be magnified along with it.
    tree.chrome = False
    body = ws.insert_node_dyn(tree)

    (columns, grouped, families) = annotation_columns(frame.columns)
    rings = ws.insert_node_dyn(
        AnnotationRings(body, columns, ring_palette(grouped, families)))
    button = dex.Button.build(ws, dex.Label.new("Open genome"))
    panel = ws.insert_node_dyn(TreePanel(body, button))
    # The rings first, so the panel sits over them.
    return on_plane(ws, body, (TREE_SIZE, TREE_SIZE), name="Phylogeny",
                    foreground=[rings, panel], what="Placed the tree")


def _find_table():
    for value in globals().values():
        if type(value).__name__ in ("RecordBatch", "Table"):
            return value
    return None


def transform():
    batch = _find_table()
    if batch is None:
        raise ValueError("wire the phylogeny Table into this transform")
    return build(dex.ws, batch)
