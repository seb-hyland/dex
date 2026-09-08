"""A phylogenetic tree from a column of lineage strings.

Wire a `Table` into a lambda argument named `thisData` and this reads a column
whose values are semicolon-separated lineages —

    Bacteria;Proteobacteria;Gammaproteobacteria;Escherichia;Escherichia coli

— and draws the tree they share, on a plane you can pan and zoom.

Almost all of it is the workspace prelude's, which is the point of this example.
`Phylogeny` is a library layout; `build_plot` gives it the sensor it picks with;
`plot_on_plane` puts it on a surface of its own with its readout pinned in the
foreground, so hovering a node names its lineage and clicking one shows the whole
record as a table, legible however far the plane is zoomed in.

What is left here is the only part that is about *this* data: which column holds
the lineages, and which one to colour the clades by. Everything else — the
picking, the readout, the overlay, and the whole query protocol that lets this
tree be joined to any other view — comes with the layout.

Set `tree_shape` to `"circular"` for the same tree bent round a circle. It is one
node with two shapes rather than two layouts, so both place their rows
identically and a link drawn to either lands in the same place.
"""


def lineage_column(frame, named=None):
    """The column holding the lineages.

    Named if you name one; otherwise the categorical column whose values look
    most like lineages — the one with the most semicolons in it, which is a
    better guess than the first column and costs one pass over the data.
    """
    if named in frame.columns:
        return named
    best = None
    for column in frame.columns:
        depth = max((str(v).count(";") for v in frame.values(column)[:64]
                     if v is not None), default=0)
        if depth and (best is None or depth > best[1]):
            best = (column, depth)
    return best[0] if best else (frame.columns[0] if frame.columns else None)


def clade_column(frame, lineage):
    """A column to colour by, or `None`.

    It has to *group*: more than one value, and comfortably fewer values than
    there are rows. A column with a distinct value per row — an accession, a
    species name — passes a naive "is it categorical" test and then colours
    every branch differently, which says nothing.
    """
    ceiling = min(24, max(2, frame.n // 2))
    for column in frame.categorical():
        if column == lineage:
            continue
        if 1 < len(frame.levels(column)) <= ceiling:
            return column
    return None


def transform():
    """The tree of `thisData`'s lineages, on a plane of its own."""
    source = globals().get("thisData")
    frame = Frame(source)
    lineage = lineage_column(frame, globals().get("lineage_column"))
    if lineage is None:
        raise ValueError("wire a table with a lineage column into this transform")

    tree = build_plot(
        dex.ws, Phylogeny,
        frame=frame,
        x=lineage,
        color=clade_column(frame, lineage),
        shape=globals().get("tree_shape") or "hierarchical",
    )
    return plot_on_plane(dex.ws, tree, name="Phylogeny")
