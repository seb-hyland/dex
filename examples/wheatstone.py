"""A Wheatstone bridge, solved by being a bridge.

Nothing here integrates a circuit. The transform runs once and lays a schematic
on a canvas: four voltages, five resistances, and eight lambdas wired to each
other exactly the way the components they stand for are wired to each other.
Then it settles, and what it settles on is the circuit's answer.

The graph is not a picture *of* the circuit. It is the circuit, and the two
kinds of lambda on it are the two kinds of physical law:

  * **A component is a lambda**, and a lambda is a function — which is what a
    resistor is. It reads the voltage at each of its terminals, divides the
    difference by its resistance, and its value is the current through it.
    Ohm's law, as a function of two numbers and a rating.

  * **A junction is a lambda too, and its law is Kirchhoff's.** The currents
    into a net must cancel, every one of them is a difference of potential over
    a resistance, and so the voltage that cancels them is the average of its
    neighbours' — weighted by how well this net is connected to each:

        v = sum(v_k / r_k) / sum(1 / r_k)

    which is the nodal equation solved for the one unknown it is about. There
    is no step size in it to be chosen badly. Edit any arm to any value and it
    still converges, in about a dozen passes.

Every number has exactly one thing that determines it: an arm's current is its
own lambda's output, and a net's voltage is written by the one junction lambda
that owns it. So two lambdas can never race for the same number — which matters
more than it sounds, because a number here takes only an absolute write, and the
loser of a race is simply lost. The cycle is real: the two junctions read each
other through the arm between them, and go round until they agree.

**It stops because a declaration says so.** Every junction takes `go` — the
total current failing to balance anywhere in the circuit, which one small
lambda watches — and declares it `satisfying go > SETTLED`. Once the bridge is
solved nothing is out of balance, the declaration refuses, and because a lambda
only runs when something it reads has moved, the whole graph goes quiet and
costs nothing at all. This is `factorial.py`'s base case doing a job: there is
no `if` in this file's circuit either.

Residual is the right thing to watch and slowness would have been the wrong
one. Edit a resistance and the current through it is instantly wrong, so the
imbalance jumps, so the declaration passes again and the bridge re-solves. A
machine that stopped when things stopped *moving* would have had no way back.

**One thing this deliberately is not.** An earlier draft had each junction nudge
its own voltage by the current arriving at it — which reads more like a circuit
charging up, and is wrong here for a reason worth keeping. To write its own
voltage a lambda must also read it, and a lambda re-runs when anything it reads
moves. So it re-fired on its own write, while its neighbours' currents still
described the voltage it had *before* the correction, and applied that same
correction again. It does not wobble or drift: it detonates, doubling every few
passes. What it takes to be safe is what is written above — an answer that
depends only on what the neighbours say, so arriving at it twice is arriving at
it once.

**What to do with it.** It arrives at rest holding no current anywhere, and a
lambda records what its inputs are worth before it ever fires on them — so,
like `factorial.py`, it wants one nudge to start: press Run, or edit any number
on the canvas. The bridge is unbalanced as built, and `R5` — the arm across the
middle, where a galvanometer goes — carries a current saying so. Balance is
`R1/R2 = R3/R4`, so edit `R4` to 300 and watch the current through the middle
fall to nothing. That is the whole of what a Wheatstone bridge is for: you do
not measure the unknown arm, you null the detector and read the answer off the
ratio of the other three.
"""

#: The supply, in volts, across the top of the bridge.
SUPPLY = 5.0

#: The five arms, in ohms. `R5` is the detector arm across the middle; the
#: bridge balances when `R1/R2 == R3/R4`, which these deliberately do not — and
#: all five differ, so each is easy to pick out on the canvas.
ARMS = {"R1": 100.0, "R2": 200.0, "R3": 150.0, "R4": 120.0, "R5": 300.0}

#: Total unbalanced current, in amps, below which the bridge is called solved.
#: Far under anything a real detector would see, and reached in about a dozen
#: passes, so there is no reason to ask for less.
SETTLED = 1e-9

# A lambda needs room for its editor and its output row or the row never draws,
# and a wire can only reach something that drew — so a component's current,
# which is its output and what everything downstream reads, would be
# unreachable in a shorter card.
LAMBDA_SIZE = (420.0, 340.0)
NUMBER_SIZE = (90.0, 32.0)


# =======================================================================
# The circuit
# =======================================================================
#
# Read as a diamond: `supply` at the top, `ground` at the foot, `left` and
# `right` the two midpoints, and `R5` the arm between them. Each arm names the
# net its current flows *from* and the one it flows *to*, and that convention
# is the whole of the bookkeeping.

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

# The canvas *is* the schematic, so the layout is the drawing: supply along the
# top, ground along the foot, the two midpoints out at the sides, and the five
# arms between them in the diamond a bridge is always drawn as.
#
# Everything below is in canvas units, and the only rule that matters is that
# nothing overlaps: an arm's card is 420x340, and a net tucked inside one of
# those rectangles is drawn *under* it and cannot be seen or grabbed.

#: Where each net's number sits: the two rails, and the two midpoints.
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
#
# A pure function of two voltages and a rating. No guard, because there is
# nothing to guard: it recomputes when a voltage it reads moves, and stops when
# they stop.
COMPONENT_SOURCE = '''def transform():
    """The current through this arm, from its first terminal to its second."""
    return (v_from - v_to) / ohms
'''

# One junction, as its lambda runs it.
#
# Kirchhoff's law, solved for this net's own potential. Idempotent on purpose:
# see the note in the module docstring about the version that was not.
NET_SOURCE = '''def transform():
    """Set this junction's voltage: its neighbours', weighted by conductance."""
    settled = sum(v / r for (v, r) in NEIGHBOURS) / sum(1.0 / r for (_, r) in NEIGHBOURS)
    dex.ws.submit_action(dex.args.v, dex.SetText(repr(settled)), "Solved a junction")
    return settled
'''

# The watcher, as its lambda runs it.
#
# The one lambda with no guard on it, and that is deliberate: it is what
# notices the circuit has been disturbed, so it has to be able to run when
# everything else has refused to. Its value is the residual every junction is
# declared against.
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
    """A lambda on the canvas with its arguments declared and wired.

    `declarations` is `(label, name, kind, detail, wired)` per argument, the
    same shape an arguments row is built from. `header` is prepended to the
    script, which is how a junction is told which of its arguments go together:
    the wiring says where the numbers come from, and the header says what they
    mean.
    """
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
    """One junction, which writes the voltage that balances it.

    `neighbours` is `(voltage, resistance)` per arm on this net — the voltage at
    the far end of it, and the arm between, which is all Kirchhoff's law needs
    to be told.

    `v` is this net's own number, and the script never reads it: it is an
    argument only because writing a node means naming it, and `dex.args` names
    arguments. So this does re-fire on its own write — and that is harmless
    exactly because the answer is absolute. Working it out twice from unchanged
    neighbours works out the same number twice.
    """
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
    """The imbalance across the whole circuit, which is what wakes it up.

    Wired to every current, and declaring nothing — a guard here would be a
    machine that cannot notice it has been disturbed.
    """
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
