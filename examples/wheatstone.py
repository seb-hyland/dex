"""A Wheatstone bridge, solved by being a bridge.
  * A component is a lambda.
  * A resistor is a lambda that applies Ohm's law
  * A junction is a lambda that applies a Kirchhoff's law best-guess until steady-state is reached
"""

SUPPLY = 5.0
ARMS = {"R1": 100.0, "R2": 200.0, "R3": 150.0, "R4": 120.0, "R5": 300.0}

SETTLED = 1e-9

LAMBDA_SIZE = (420.0, 340.0)
NUMBER_SIZE = (90.0, 32.0)


# =======================================================================
# The circuit
# =======================================================================

#: `(name, from net, to net)`. Positive current runs from the first to the second.
COMPONENTS = [
    ("R1", "supply", "left"),
    ("R2", "left", "ground"),
    ("R3", "supply", "right"),
    ("R4", "right", "ground"),
    ("R5", "left", "right"),
]

#: The junctions that solve themselves. `supply` and `ground` are held, not solved.
FREE_NETS = ["left", "right"]

NET_PLACES = {
    "supply": (760.0, 0.0),
    "left": (120.0, 620.0),
    "right": (1600.0, 620.0),
    "ground": (760.0, 1320.0),
}

#: Where each arm's lambda sits, as the schematic draws it.
ARM_PLACES = {
    "R1": (180.0, 120.0),
    "R3": (1180.0, 120.0),
    "R5": (680.0, 520.0),
    "R2": (180.0, 900.0),
    "R4": (1180.0, 900.0),
}


# One arm, as its lambda runs it.
COMPONENT_SOURCE = '''def transform():
    """The current through this arm, from its first terminal to its second."""
    return (v_from - v_to) / ohms
'''

# One junction, as its lambda runs it.
NET_SOURCE = '''def transform():
    """Set this junction's voltage: its neighbours', weighted by conductance."""
    settled = sum(v / r for (v, r) in NEIGHBOURS) / sum(1.0 / r for (_, r) in NEIGHBOURS)
    dex.ws.submit_action(dex.args.v, dex.SetText(repr(settled)), "Solved a junction")
    return settled
'''

# The watcher, as its lambda runs it.
MONITOR_SOURCE = '''def transform():
    """Total current failing to balance anywhere in the circuit."""
    return sum(abs(sum(sense * i for (sense, i) in net)) for net in NETS)
'''


def place(ws, canvas, node, at, size):
    """Put `node` on `canvas` at `at`; returns the item wrapping it.

    The item is what a wire points at — a canvas draws its children as the
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
    """An editable number on the canvas; returns the item wrapping it."""
    node = dex.NodeUid.mint()
    ws.insert_node_at_dyn(node, float(value))
    return place(ws, canvas, node, at, NUMBER_SIZE)


def build_lambda(ws, canvas, name, source, declarations, at, header=""):
    """A lambda on the canvas with its arguments declared and wired."""
    uid, args, output = dex.NodeUid.mint(), dex.NodeUid.mint(), dex.NodeUid.mint()
    ws.insert_node_at_dyn(
        uid, dex.Lambda.new_with(ws, args, output, name, header + source)
    )
    # A lambda's output starts empty, and empty is not a number: anything wired
    # to a lambda that has not run yet would be handed nothing, fail the
    # declaration saying it takes a number, and never run — so nothing upstream
    # could run either, and the circuit would have no way to start. Zero is also
    # the honest initial condition: at the instant it is built, nothing flows.
    ws.insert_node_at_dyn(output, 0.0)
    for (label, arg_name, kind, detail, wired) in declarations:
        arg, port = dex.NodeUid.mint(), dex.NodeUid.mint()
        dex.LambdaArg.build_with(ws, arg, port, label, arg_name)
        ws.submit_action(args, dex.AddArgAt(arg))
        ws.submit_action(arg, dex.SetArgKind(kind, detail))
        ws.submit_action(port, dex.SetConnection(wired))
    place(ws, canvas, uid, at, LAMBDA_SIZE)
    return output


def component_lambda(ws, canvas, name, v_from, v_to, ohms, at):
    """One arm: reads both its terminals, and its value is the current."""
    return build_lambda(
        ws,
        canvas,
        name,
        COMPONENT_SOURCE,
        [
            ("from", "v_from", dex.ArgType.Float, "", v_from),
            ("to", "v_to", dex.ArgType.Float, "", v_to),
            ("ohms", "ohms", dex.ArgType.Float, "", ohms),
        ],
        at,
    )


def net_lambda(ws, canvas, name, voltage, neighbours, go, at):
    """One junction, which writes the voltage that balances it."""
    declarations = [
        ("volts", "v", dex.ArgType.Float, "", voltage),
        # The guard, and the only reason this ever stops. It names the residual
        # rather than this junction's own movement: a net that has stopped
        # moving because its neighbours have is not the same as a solved
        # circuit, and nothing that stopped for the first reason could start.
        ("while", "go", dex.ArgType.Satisfying, "go > %r" % SETTLED, go),
    ]
    for (index, (voltage, ohms)) in enumerate(neighbours):
        declarations.append(("volts", "v%d" % index, dex.ArgType.Float, "", voltage))
        declarations.append(("ohms", "r%d" % index, dex.ArgType.Float, "", ohms))
    # The names are bound positionally in a header rather than handed over as a
    # list, because an argument is one wire and a list is not a thing a wire
    # can carry.
    header = "NEIGHBOURS = [%s]\n" % ", ".join(
        "(v%d, r%d)" % (index, index) for index in range(len(neighbours))
    )
    return build_lambda(ws, canvas, name, NET_SOURCE, declarations, at, header)


def monitor_lambda(ws, canvas, currents, at):
    """The imbalance across the whole circuit, which is what wakes it up."""
    order = {name: index for (index, name) in enumerate(sorted(currents))}
    declarations = [
        ("current", "i%d" % index, dex.ArgType.Float, "", currents[name])
        for (name, index) in sorted(order.items(), key=lambda pair: pair[1])
    ]
    groups = []
    for net in FREE_NETS:
        terms = [
            "(%d, i%d)" % (-1 if source == net else 1, order[name])
            for (name, source, sink) in COMPONENTS
            if net in (source, sink)
        ]
        groups.append("[%s]" % ", ".join(terms))
    header = "NETS = [%s]\n" % ", ".join(groups)
    return build_lambda(
        ws, canvas, "Imbalance", MONITOR_SOURCE, declarations, at, header
    )


def build(ws):
    """The bridge, as a canvas lambda; returns its uid."""
    # Every id up front: the queue does not drain until this returns, so nothing
    # built here can be looked up afterwards.
    root, args = dex.NodeUid.mint(), dex.NodeUid.mint()
    canvas, output_port = dex.NodeUid.mint(), dex.NodeUid.mint()
    ws.insert_node_at_dyn(
        root,
        dex.CanvasLambda.new_with(ws, args, canvas, output_port, "Wheatstone bridge"),
    )

    # The four nets, every one a stored number. Two are held at what they are —
    # which is what a source is, and editing one changes what the bridge is
    # being asked — and two are written by the junction lambdas below.
    #
    # They have to be *stored* rather than each junction's output, and that is
    # not a detail. A lambda's output belongs to the lambda: it is emptied the
    # moment the lambda has anything to say, including that its own arguments
    # are not ready. Two junctions that read each other would then each be
    # waiting on a number the other has just cleared, and the circuit would
    # never start at all. A number on the canvas is under no such obligation.
    nets = {
        name: number(ws, canvas, SUPPLY if name == "supply" else 0.0, at)
        for (name, at) in NET_PLACES.items()
    }
    # The five ratings, in a row beneath the schematic. These are the circuit's
    # parameters and the thing to edit: every one is a plain number.
    ohms = {
        name: number(ws, canvas, value, (120.0 + 340.0 * index, 1560.0))
        for (index, (name, value)) in enumerate(sorted(ARMS.items()))
    }

    currents = {
        name: component_lambda(
            ws, canvas, name, nets[source], nets[sink], ohms[name], ARM_PLACES[name]
        )
        for (name, source, sink) in COMPONENTS
    }

    go = monitor_lambda(ws, canvas, currents, (680.0, 1700.0))

    for (index, net) in enumerate(FREE_NETS):
        neighbours = [
            (nets[sink if source == net else source], ohms[name])
            for (name, source, sink) in COMPONENTS
            if net in (source, sink)
        ]
        at = (120.0 + 1060.0 * index, 2120.0)
        net_lambda(ws, canvas, "Junction %s" % net, nets[net], neighbours, go, at)

    # What the bridge is worth reading off: the current through the middle. It
    # is zero exactly when the bridge is balanced, which is the measurement.
    ws.submit_action(output_port, dex.SetConnection(currents["R5"]))
    return root


def transform():
    """The bridge, built and left to solve itself."""
    return build(dex.ws)
