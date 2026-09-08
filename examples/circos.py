"""A circos plot of the chord kind: a ring of groups, joined by ribbons.

Wire a `Table` into `thisData` where each row names a thing and the thing it
points at — a contig and the contig it links to, a gene and its ortholog, a
country and where its exports go. This lays the rows out around a ring, grouped
by one column, and draws a ribbon from every row to whichever row its link
column names.

All of that is the prelude's `Circos`. The example is the *arguments*:

  * `x` — the column that divides the ring into sectors. Each sector is as wide
    as the rows in it, so the ring is a picture of the whole table.
  * `key` — the column that identifies a row, which is what a link points at.
  * `link` — the column holding another row's key. Every one becomes a chord.
  * `tracks` — continuous columns drawn as rings outside, each against its own
    range, because a track is read against itself.

Because it is a library layout it answers the same protocol everything else
does: hover a row for its values, click for the whole record as a table, and
wire it beside a scatter to draw lines between the same record in both.

With nothing wired it invents a small set of linked contigs, so it opens onto a
picture rather than an error.
"""

#: The plot is drawn once at this size and then panned and zoomed.
PLOT_SIZE = (1000.0, 1000.0)


def sample():
    """A handful of contigs, each linking to another, with two measurements."""
    import random

    rng = random.Random(11)
    groups = ["chr1", "chr2", "chr3", "chr4"]
    names = ["ctg%02d" % i for i in range(28)]
    return {
        "contig": names,
        "chromosome": [groups[i % len(groups)] for i in range(len(names))],
        # Each links to a contig some way round the ring, so the chords cross.
        "links_to": [names[(i * 7 + 5) % len(names)] for i in range(len(names))],
        "depth": [round(rng.uniform(8.0, 90.0), 1) for _ in names],
        "gc": [round(rng.uniform(28.0, 68.0), 1) for _ in names],
    }


def guess_columns(frame):
    """`(group, key, link, tracks)` — which column plays which part.

    Named explicitly if you name them; otherwise: the key is the first column
    whose values are all distinct, the link is the next column that holds *keys*
    rather than new values, the group is the categorical column that actually
    groups, and the tracks are whatever is continuous.
    """
    key = globals().get("key_column")
    link = globals().get("link_column")
    group = globals().get("group_column")

    if key not in frame.columns:
        key = next((c for c in frame.categorical()
                    if len(frame.levels(c, cap=frame.n + 1)) == frame.n), None)
    if link not in frame.columns and key:
        known = set(frame.values(key))
        link = next((c for c in frame.categorical()
                     if c != key and known.issuperset(
                         v for v in frame.values(c) if v is not None)), None)
    if group not in frame.columns:
        group = next((c for c in frame.categorical()
                      if c not in (key, link) and 1 < len(frame.levels(c)) < frame.n), None)
    return (group, key, link, frame.continuous()[:2])


def transform():
    """The chord plot of `thisData`, on a plane of its own."""
    frame = Frame(globals().get("thisData") or sample())
    (group, key, link, tracks) = guess_columns(frame)
    if group is None:
        raise ValueError("this wants a column that groups the rows into sectors")

    plot = build_plot(
        dex.ws, Circos,
        frame=frame,
        x=group,
        key=key,
        link=link,
        tracks=tracks,
    )
    return plot_on_plane(dex.ws, plot, PLOT_SIZE, "Circos")
