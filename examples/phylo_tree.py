"""A taxonomic dendrogram, with a rank axis pinned over it.

Give it a `Table` with a column of the format every metagenomics tool speaks —

    d__Bacteria;p__Bacillota;c__Bacilli;...;s__Streptococcus pneumoniae

— wired into a lambda argument named `table`, and it draws the tree those
lineages share as a dendrogram lying on its side: depth runs left to right, one
column per rank, taxa stacked down the page with each parent centred on its
children. The prefixes (`d__`, `p__`) come off in the prelude's `split_lineage`,
so the same lineage makes the same tree whether or not the ranks were labelled.

The tree itself is the prelude's `Phylogeny` in its `"sideways"` shape — the
same node `phylogeny.py` draws down the page and round a circle, so all three
place their rows identically and any of them links to any other view.

What this example adds is the chrome: a `RankAxis` that bands a column per rank
and heads it. It lives in the plane's **foreground**, which is what makes the
headings stay put and stay legible while the tree behind them is panned and
magnified — and it reads the rank count off the tree rather than being told, so
it cannot fall out of step with what is drawn.
"""

#: The tree is drawn once at this size and then panned; generous, so the long
#: species names at the right have room to breathe.
TREE_SIZE = (1500.0, 1100.0)

RANK_NAMES = ("domain", "phylum", "class", "order", "family", "genus", "species")
BAND = (247, 249, 252)
BAND_ALT = (255, 255, 255)
HEADING_FONT = 11.0


class RankAxis:
    """A band per rank behind the tree, with the rank's name over it.

    Pinned in the plane's foreground, so the headings hold their size and their
    place while the tree moves under them. It asks the tree how deep it goes
    rather than being told, so adding a rank to the data cannot leave the axis
    describing the tree it used to be.
    """

    def __init__(self, tree):
        self.tree = tree

    def owned_nodes(self):
        return []

    def type_name(self):
        return "A Rank Axis"

    def draw(self, ctx):
        base = ctx.constraints
        w = base.x.provided_value() if base.x is not None else None
        h = base.y.provided_value() if base.y is not None else None
        if w is None or h is None:
            return dex.DrawResult.Complete(region=None)
        (x, y) = (base.pos.x, base.pos.y)

        ranks = ctx.node.workspace.send_request(self.tree, RankCount()) or 1
        step = w / float(ranks + 1)
        for k in range(ranks + 1):
            left = x + step * k
            # Alternating grounds, so a column is told from its neighbour
            # without a rule between every pair.
            rect(ctx, left, y, step, h, BAND if k % 2 == 0 else BAND_ALT, alpha=200)
            caption = RANK_NAMES[k] if k < len(RANK_NAMES) else "rank %d" % (k + 1)
            (cw, _ch) = measure(ctx, caption, HEADING_FONT)
            text(ctx, caption, left + (step - cw) / 2.0, y + 6.0,
                 HEADING_FONT, FAINT, bold=True)
        return dex.DrawResult.Complete(
            region=dex.ScreenRegion.from_min_size(base.pos, dex.Vector.new(w, h)))


class RankCount(dex.Request):
    """How many ranks deep a tree goes."""

    name = "dex.example.rank_count"


class RankedTree(Phylogeny):
    """The library's tree, answering one question of its own.

    A layout is an ordinary class: subclass it, add a message, and everything
    else — the picking, the readout, the whole protocol — is still there. Which
    is the point of the tree being a library node rather than an example's own.
    """

    def request(self, req, ctx):
        if getattr(req, "name", None) == RankCount.name:
            return self.ranks()
        return super().request(req, ctx)


def transform():
    """The dendrogram of `table`'s lineages, under a rank axis."""
    ws = dex.ws
    source = globals().get("table") or globals().get("thisData")
    frame = Frame(source)

    lineage = None
    for column in frame.columns:
        if any(";" in str(v) for v in frame.values(column)[:32] if v is not None):
            lineage = column
            break
    if lineage is None:
        raise ValueError("wire a table with a lineage column into this transform")

    tree = build_plot(ws, RankedTree, frame=frame, x=lineage, shape="sideways")
    tree.chrome = False
    body = ws.insert_node_dyn(tree)
    # Both in the foreground, and the axis first, so the readout sits over it.
    axis = ws.insert_node_dyn(RankAxis(body))
    readout = ws.insert_node_dyn(PlotChrome(body))
    return on_plane(ws, body, TREE_SIZE, "Taxonomy", [axis, readout])
