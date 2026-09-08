"""The workspace prelude: one query protocol every visualization speaks.

Paste this into the Settings tab's global prelude. It runs before every lambda,
so the message classes below are in scope wherever a transform is written, and —
because dex now lets a prelude define its own messages — any node may send them
to any other with `dex.send_request`.

The problem it solves: two plots built from the same table are two separate
nodes that share nothing but the rows behind them. To draw a line between the
same record in a scatter and in a strip plot, one view has to be able to ask the
other *where did you draw row 7 this frame, and what is in it* — without knowing
what kind of plot the other is. That is what these four messages are. Every plot
in `data_explorer.py` and `phylogeny.py` answers them, so a third view can wire
to either and treat them alike.

A message is matched by its `name` tag, never by class identity. A transform
defines its classes afresh each run, and saving or cloning a workspace gives
them new identities again; an `isinstance` check across those boundaries would
silently miss. A short string does not. So every answering node keys off
`getattr(req, "name", None)`, and the tags here are the whole contract — kept in
one place, prefixed so nothing else collides with them.

Coordinates are screen pixels for the frame in which the answer is given: "where
it is drawn *this frame*". A querying node must therefore be drawn after the plot
it asks — later in the same layer, or in a layer above it — so the plot has
recorded the frame before the question is asked.

    ROW_KEYS     -> [row_id, ...]              every row the plot can place
    DRAWN_POINTS -> {row_id: (x, y), ...}      every row drawn this frame
    DRAWN_POINT  -> (x, y) | None              where one row is, this frame
    ROW_VALUES   -> {column: value} | None     the record behind a row

`row_id` is the row's index in the source table, so the same id means the same
record in every view built from that table.
"""

# The tags. A plot and its questioner share nothing else, so these strings are
# the contract; prefixed to stay clear of any other message.
ROW_KEYS = "dex.plot.row_keys"
DRAWN_POINTS = "dex.plot.drawn_points"
DRAWN_POINT = "dex.plot.drawn_point"
ROW_VALUES = "dex.plot.row_values"


class RowKeys(dex.Request):
    """Every row id this plot can place, in the order it draws them."""

    name = ROW_KEYS


class DrawnPoints(dex.Request):
    """Every row drawn this frame, as `{row_id: (screen_x, screen_y)}`.

    Empty until the plot has drawn once, and only the rows actually on screen
    this frame — a plot that shows one column at a time answers for the rows in
    view, not the whole table.
    """

    name = DRAWN_POINTS


class DrawnPoint(dex.Request):
    """Where a single row is drawn this frame, as `(screen_x, screen_y)`, or
    `None` if it is not on screen."""

    name = DRAWN_POINT

    def __init__(self, row_id):
        self.row_id = row_id


class RowValues(dex.Request):
    """The record behind a row id, as a `{column: value}` dict, or `None`."""

    name = ROW_VALUES

    def __init__(self, row_id):
        self.row_id = row_id
