"""The other circos idiom: concentric tracks around one assembly.

`circos.py` is the chord kind — groups joined by ribbons. This is the one you
get out of an assembly report: the ring is the assembly itself, divided into its
contigs, and every ring outside it is a question asked of each contig. Depth,
GC, gene density, N-content: one track each, and you read down through them at a
fixed angle to see everything known about one piece.

It is the *same* `Circos` layout as `circos.py`, given tracks instead of links —
which is the point worth taking from this example. The two idioms are not two
pieces of code; they are one layout with different columns pointed at it. Add a
`link` column here and you get ribbons too.

A track is scaled against its own range, never a shared one. Two columns in
different units forced onto one axis is how a circos stops saying anything: the
one with the bigger numbers fills its ring and the other lies flat.

With nothing wired it invents an assembly, so it opens onto a picture.
"""

PLOT_SIZE = (1000.0, 1000.0)

#: A track per column, in from the ring outward. More than about five and the
#: rings get too thin to read at life size.
MAX_TRACKS = 5


def sample():
    """An assembly: contigs in bins, with the usual QC measurements."""
    import random

    rng = random.Random(3)
    contigs = ["ctg%03d" % i for i in range(48)]

    def bin_of(i):
        return "bin%d" % (i // 12 + 1)

    return {
        "contig": contigs,
        "bin": [bin_of(i) for i in range(len(contigs))],
        "length_kb": [round(rng.uniform(12.0, 900.0), 1) for _ in contigs],
        "depth": [round(rng.uniform(4.0, 120.0), 1) for _ in contigs],
        "gc": [round(rng.uniform(29.0, 67.0), 1) for _ in contigs],
        "genes": [rng.randint(8, 820) for _ in contigs],
    }


def transform():
    """The QC rings of `thisData`, on a plane of its own."""
    frame = Frame(globals().get("thisData") or sample())

    group = globals().get("group_column")
    if group not in frame.columns:
        # The column that divides the assembly into pieces: categorical, and
        # coarser than one value per row.
        group = next((c for c in frame.categorical()
                      if 1 < len(frame.levels(c)) < frame.n), None)
    if group is None:
        raise ValueError("this wants a column that groups the rows into sectors")

    plot = build_plot(
        dex.ws, Circos,
        frame=frame,
        x=group,
        tracks=frame.continuous()[:MAX_TRACKS],
    )
    return plot_on_plane(dex.ws, plot, PLOT_SIZE, "Assembly QC")
