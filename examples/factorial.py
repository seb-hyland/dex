"""A factorial, computed by a lambda that feeds itself until it is told to stop."""

STEP_SOURCE = '''def transform():
    """Fold `n` into `acc`, then step `n` down. Runs only while `n > 0`."""
    product = acc * n
    dex.ws.batch(
        [
            (dex.args.acc, dex.SetText(str(product))),
            (dex.args.n, dex.SetText(str(n - 1))),
        ],
        "Factorial step",
    )
    return product
'''

# A lambda needs room for its editor and its output row or the row never draws,
# and a wire can only reach something that drew.
STEP_SIZE = (420.0, 340.0)
NUMBER_SIZE = (90.0, 32.0)


def place(ws, canvas, node, at, size):
    """Put `node` on `canvas` at `at`; returns the item wrapping it.

    The item is what a wire points at — the canvas draws its children as the
    addressable things, not what they wrap — and it stands for its child's
    value, so an argument wired to the item reads the number underneath.
    """
    item = dex.CanvasNode.build(
        ws,
        node,
        dex.Vector.new(at[0], at[1]),
        dex.Vector.new(size[0], size[1]),
    )
    ws.submit_action(canvas, dex.AdoptCanvasNode(item, dex.Layer.midground()))
    return item


def number(ws, canvas, value, at):
    """An editable integer on the canvas; returns the item wrapping it."""
    node = dex.NodeUid.mint()
    ws.insert_node_at_dyn(node, value)
    return place(ws, canvas, node, at, NUMBER_SIZE)


def step_lambda(ws, canvas, counter, total):
    """The lambda that runs one step, wired to `counter` and `total`."""
    uid, args, output = dex.NodeUid.mint(), dex.NodeUid.mint(), dex.NodeUid.mint()
    ws.insert_node_at_dyn(
        uid,
        dex.Lambda.new_with(ws, args, output, "Step", STEP_SOURCE),
    )

    declarations = [
        ("count", "n", dex.ArgType.Satisfying, "n > 0", counter),
        ("product", "acc", dex.ArgType.Int, "", total),
    ]
    for label, name, kind, detail, wired in declarations:
        arg, port = dex.NodeUid.mint(), dex.NodeUid.mint()
        dex.LambdaArg.build_with(ws, arg, port, label, name)
        ws.submit_action(args, dex.AddArgAt(arg))
        ws.submit_action(arg, dex.SetArgKind(kind, detail))
        ws.submit_action(port, dex.SetConnection(wired))

    place(ws, canvas, uid, (0.0, 90.0), STEP_SIZE)
    return output


def build(ws, start):
    """A canvas lambda counting `start` down to zero; returns its uid."""
    # Every id up front: the queue does not drain until this returns, so nothing
    # built here can be looked up afterwards.
    root, args = dex.NodeUid.mint(), dex.NodeUid.mint()
    canvas, output_port = dex.NodeUid.mint(), dex.NodeUid.mint()

    # No parameters. The machine's inputs are the two numbers on its own
    # surface, which is what makes it a closed loop rather than a function of
    # something outside it.
    ws.insert_node_at_dyn(
        root,
        dex.CanvasLambda.new_with(ws, args, canvas, output_port, "%d!" % start),
    )

    counter = number(ws, canvas, start, (0.0, 0.0))
    total = number(ws, canvas, 1, (140.0, 0.0))
    step_lambda(ws, canvas, counter, total)

    # The accumulator is the result. The step lambda's own output is only the
    # running product, and it ends as the complaint that stopped the recursion.
    ws.submit_action(output_port, dex.SetConnection(total))
    return root


def transform():
    """The machine, counting down from whatever is wired into `n`."""
    return build(dex.ws, int(n))
