"""Symbolic differentiation of a canvas lambda.

Bind a `CanvasLambda` to `f` and the name of one of its parameters to `var`.
The transform reads `f`'s inner graph, differentiates it with respect to that
parameter, and returns a new canvas lambda computing the derivative.

Operators are recognised by their display name, so an inner lambda called
`Mult` is a product and one called `Frobnicate` is an error rather than a
silent zero. A nested canvas lambda is descended into.

Nothing is simplified: every rule application becomes real nodes, so `* 1` and
`+ 0` terms survive. The graph is bigger that way, but it maps one-for-one onto
the rules below, which is what makes it checkable by eye.
"""

ADD, SUB, MULT, DIV, POW = "add", "sub", "mult", "div", "pow"

# Recognised operator names, normalised. The user's own lambdas are the real
# definitions; these are only the fallbacks used when the source canvas has no
# example of an operator the derivative needs.
PRIMITIVE_SOURCE = {
    ADD: "def transform():\n    return a + b\n",
    SUB: "def transform():\n    return a - b\n",
    MULT: "def transform():\n    return a * b\n",
    DIV: "def transform():\n    return a / b\n",
    POW: "def transform():\n    return a ** b\n",
}

# An operator is a `Lambda`, and a lambda's column needs room for its editor,
# its buttons and its output row. Laid out shorter than this the column runs out
# of height and stops before the output draws — which leaves it unrecorded, and
# a wire can only be drawn to something the halo recorded. Matches the size the
# sidebar builds a lambda at.
OP_SIZE = (420.0, 340.0)
CONST_SIZE = (60.0, 30.0)

# The derivative is laid out as a tree: one column per level, operands to the
# left of what consumes them, so the graph reads in the direction the values
# flow. Placing without this would stack every node on one spot, because
# `PlaceOnCanvas` centres whatever it is given in the viewport.
COLUMN_W = 480.0
ROW_H = 380.0
# Each step towards the result also drops, so a chain reads diagonally rather
# than as a flat row: you can follow it without tracing which wire goes where.
STEP_DOWN = 110.0
# Clearance under the parameter pins, and under anything of the original the
# derivative still points at.
BAND_GAP = 80.0
PIN_CLEARANCE = 140.0


def normalise(name):
    """An operator's identity: what the user typed, ignoring case and padding."""
    return (name or "").strip().lower()


# ======================================================================
# The expression tree
# ======================================================================


class Expr:
    """A node of the read graph, remembering where it came from.

    `source` is the uid this was read from, which is what lets the emitter
    point at the *copy* of an original sub-expression instead of rebuilding it.
    """

    def __init__(self, source):
        self.source = source


class Const(Expr):
    def __init__(self, text, source=None):
        super().__init__(source)
        self.text = text

    def __repr__(self):
        return f"Const({self.text})"


class Var(Expr):
    """A parameter pin of the lambda being differentiated."""

    def __init__(self, index, source=None):
        super().__init__(source)
        self.index = index

    def __repr__(self):
        return f"Var({self.index})"


class Op(Expr):
    def __init__(self, name, operands, source=None):
        super().__init__(source)
        self.name = name
        self.operands = operands

    def __repr__(self):
        return f"{self.name}({', '.join(map(repr, self.operands))})"


class Ref(Expr):
    """The original sub-expression at `source`, reused rather than rebuilt.

    The product rule needs `u` and `v` themselves, not copies of their
    structure, and the derivative canvas begins life as a clone of the original
    — so every original node already has a counterpart there to point at.
    """

    def __repr__(self):
        return f"Ref({self.source})"


class Unknown(Exception):
    """An operator the rules below do not cover."""


# ======================================================================
# Reading the graph
# ======================================================================


def producer(snap, uid, pins):
    """Follow `uid` to the thing that actually produces a value.

    A wire may land on a canvas item, on a lambda's output slot, or straight on
    a value, so the endpoint has to be normalised before it can be read.
    """
    seen = set()
    while uid is not None:
        if uid in seen:
            raise Unknown(f"the wiring at {uid} loops")
        seen.add(uid)

        if uid in pins:
            return ("pin", uid)

        # An output slot stands for the lambda that recomputes it.
        owner = snap.owner_of(uid)
        if owner is not None and snap.send_request(owner, dex.DataflowOutput()) == uid:
            uid = owner
            continue

        if snap.send_request(uid, dex.LambdaName()) is not None:
            return ("lambda", uid)

        # A canvas item is a wrapper; what matters is what it wraps.
        child = snap.send_request(uid, dex.CanvasNodeChild())
        if child is not None:
            uid = child
            continue

        text = snap.send_request(uid, dex.GetText())
        if text is not None:
            return ("const", text)

        raise Unknown(f"nothing at {uid} produces a value")

    raise Unknown("a wire leads nowhere")


def read(snap, uid, pins, depth=0):
    """The expression `uid` computes, as a tree."""
    if depth > 64:
        raise Unknown("the graph nests too deeply to walk")

    kind, found = producer(snap, uid, pins)
    if kind == "const":
        return Const(found)
    if kind == "pin":
        return Var(pins.index(found), source=found)

    name = normalise(snap.send_request(found, dex.LambdaName()))
    inputs = snap.send_request(found, dex.DataflowInputs()) or []

    body = snap.send_request(found, dex.LambdaBody())
    if body is not None:
        # A canvas lambda: read its body, with its own pins standing for the
        # expressions wired into it from out here.
        inner_pins = snap.send_request(found, dex.ParamPins()) or []
        inner = read(snap, body, inner_pins, depth + 1)
        actuals = [
            read(snap, source, pins, depth + 1) if source is not None else Const("0")
            for (_n, _port, source) in inputs
        ]
        return substitute(inner, actuals)

    if name not in PRIMITIVE_SOURCE:
        raise Unknown(f"`{name or '<unnamed>'}` is not an operator I know")

    operands = []
    for _n, _port, source in inputs:
        if source is None:
            raise Unknown(f"`{name}` has an unwired input")
        operands.append(read(snap, source, pins, depth + 1))
    return Op(name, operands, source=found)


def substitute(expr, actuals):
    """Replace an inner lambda's parameters with what was wired into it."""
    if isinstance(expr, Var):
        return actuals[expr.index] if expr.index < len(actuals) else Const("0")
    if isinstance(expr, Op):
        return Op(expr.name, [substitute(o, actuals) for o in expr.operands], expr.source)
    return expr


# ======================================================================
# Differentiating
# ======================================================================


def constant_value(expr):
    """`expr` as a number, or `None` if it is not a literal."""
    if not isinstance(expr, Const):
        return None
    try:
        return float(expr.text)
    except ValueError:
        return None


def keep(expr):
    """The original sub-expression, to be pointed at rather than rebuilt."""
    if isinstance(expr, (Const, Var)):
        return expr
    return Ref(expr.source)


def differentiate(expr, index):
    """d(`expr`)/d(parameter `index`), with nothing folded away."""
    if isinstance(expr, Const):
        return Const("0")
    if isinstance(expr, Var):
        return Const("1" if expr.index == index else "0")
    if isinstance(expr, Ref):
        raise Unknown("a reference has no derivative of its own")

    du = [differentiate(o, index) for o in expr.operands]
    u = [keep(o) for o in expr.operands]

    if expr.name in (ADD, SUB):
        return Op(expr.name, du)
    if expr.name == MULT:
        return Op(ADD, [Op(MULT, [du[0], u[1]]), Op(MULT, [u[0], du[1]])])
    if expr.name == DIV:
        numerator = Op(SUB, [Op(MULT, [du[0], u[1]]), Op(MULT, [u[0], du[1]])])
        return Op(DIV, [numerator, Op(MULT, [u[1], u[1]])])
    if expr.name == POW:
        exponent = constant_value(expr.operands[1])
        if exponent is None:
            raise Unknown("only a constant exponent can be differentiated")
        lowered = Op(POW, [u[0], Const(number(exponent - 1))])
        return Op(MULT, [Op(MULT, [Const(number(exponent)), lowered]), du[0]])

    raise Unknown(f"`{expr.name}` has no rule")


def referenced(expr):
    """The source uids a tree still points at, rather than rebuilds."""
    found = set()
    stack = [expr]
    while stack:
        node = stack.pop()
        if isinstance(node, (Ref, Var)) and node.source is not None:
            found.add(node.source)
        elif isinstance(node, Op):
            stack.extend(node.operands)
    return found


def literal(expr):
    """`expr` as a float, if it is a constant."""
    if isinstance(expr, Const):
        try:
            return float(expr.text)
        except ValueError:
            return None
    return None


def fold(expr):
    """`expr` with the arithmetic that carries no information taken out.

    The rules emit one node per step, which is faithful but unreadable: the
    derivative of a quadratic comes out as twelve operators, nearly all of them
    multiplying by one or adding zero. Folding is not an optimisation here so
    much as the difference between a graph you can read and one you cannot.
    """
    if not isinstance(expr, Op):
        return expr

    operands = [fold(operand) for operand in expr.operands]
    left = literal(operands[0])
    right = literal(operands[1]) if len(operands) > 1 else None

    if expr.name == MULT:
        if left == 0 or right == 0:
            return Const("0")
        if left == 1:
            return operands[1]
        if right == 1:
            return operands[0]
    elif expr.name == ADD:
        if left == 0:
            return operands[1]
        if right == 0:
            return operands[0]
    elif expr.name == SUB:
        if right == 0:
            return operands[0]
    elif expr.name == POW:
        if right == 1:
            return operands[0]
        if right == 0:
            return Const("1")

    # Both sides known: do the arithmetic now rather than emitting nodes for it.
    if left is not None and right is not None:
        table = {
            ADD: left + right,
            SUB: left - right,
            MULT: left * right,
            POW: left**right,
        }
        if expr.name in table:
            return Const(number(table[expr.name]))
        if expr.name == DIV and right != 0:
            return Const(number(left / right))

    return Op(expr.name, operands, expr.source)


def number(value):
    """A literal, written as an integer when it is one."""
    return str(int(value)) if float(value).is_integer() else str(value)


# ======================================================================
# Laying out
# ======================================================================


def is_existing(expr):
    """Whether `expr` is something the clone already contains and placed."""
    return isinstance(expr, (Var, Ref))


def assign_cells(expr, depth, row, cells):
    """Give every *new* node in `expr` a `(depth, row)`; returns rows used.

    A parent is centred on the band its operands occupy, so a term sits level
    with what it is made of.
    """
    if is_existing(expr):
        return 0
    if isinstance(expr, Const):
        cells[id(expr)] = (depth, row)
        return 1

    used = 0
    for operand in expr.operands:
        used += assign_cells(operand, depth + 1, row + used, cells)
    used = max(used, 1)
    cells[id(expr)] = (depth, row + (used - 1) / 2)
    return used


def layout(expr):
    """`(depth, row)` for every node the derivative adds, keyed by identity."""
    cells = {}
    assign_cells(expr, 0, 0, cells)
    return cells


# ======================================================================
# Emitting
# ======================================================================


class Builder:
    """Builds the derivative into a clone of the lambda it differentiates.

    Starting from a clone means the derivative already has the right parameters
    and already contains every original sub-expression, so the product rule can
    point at `u` rather than reconstruct it.
    """

    def __init__(self, snap, ws, source_lambda, expr, cells):
        self.snap = snap
        self.ws = ws
        self.cells = cells
        self.clone = self.plan_clone(source_lambda)
        ws.deep_clone_as(source_lambda, list(self.clone.items()))
        self.root = self.clone[source_lambda]
        source_canvas = snap.send_request(source_lambda, dex.ComputeCanvasNode())
        self.source_canvas = source_canvas
        self.canvas = self.clone[source_canvas]
        self.templates = self.find_templates(source_lambda)
        # What the derivative still points at survives the prune, so it is also
        # what the new nodes have to keep clear of.
        self.reached = self.reachable(referenced(expr))
        self.top = self.clearance()
        self.right = max((depth for depth, _row in cells.values()), default=0)

    def clearance(self):
        """Where the derivative may start without landing on what stays.

        Under the pins, and under any of the original still being pointed at.
        Everything else on the source canvas is about to be pruned, so it does
        not get a say — leaving room for it is what used to push the derivative
        hundreds of pixels down into empty space.
        """
        bottom = PIN_CLEARANCE
        for item in self.snap.send_request(self.source_canvas, dex.CanvasChildren()) or []:
            inner = self.snap.send_request(item, dex.CanvasNodeChild())
            if item not in self.reached and inner not in self.reached:
                continue
            layout = self.snap.send_request(item, dex.CanvasNodeConstraints())
            if layout is not None:
                bottom = max(bottom, layout.pos.y + layout.size.y + BAND_GAP)
        return bottom

    def position(self, expr, size):
        """Where `expr` belongs, in canvas coordinates."""
        depth, row = self.cells[id(expr)]
        step = self.right - depth
        return (
            step * COLUMN_W,
            self.top + row * ROW_H + step * STEP_DOWN,
            size[0],
            size[1],
        )

    def place_at(self, uid, box):
        """Put `uid` on the canvas at `box`, rather than wherever it lands.

        `PlaceOnCanvas` centres what it is given, which would stack the whole
        derivative on one spot; building the item directly is what allows a
        chosen position.
        """
        x, y, width, height = box
        item = dex.CanvasNode.build(
            self.ws, uid, dex.Vector.new(x, y), dex.Vector.new(width, height)
        )
        self.ws.submit_action(self.canvas, dex.AdoptCanvasNode(item))
        return item

    def subtree(self, uid):
        """Every uid `uid` owns, transitively, including itself."""
        found, queue = [], [uid]
        seen = {uid}
        while queue:
            current = queue.pop()
            found.append(current)
            for child in self.snap.owned_refs(current):
                if child not in seen:
                    seen.add(child)
                    queue.append(child)
        return found

    def plan_clone(self, uid):
        """A new id for every node in `uid`'s subtree, chosen up front.

        `deep_clone` alone would mint these itself and report only the root,
        which is no help: the ports that have to be rewired are all inside.
        """
        return {node: dex.NodeUid.mint() for node in self.subtree(uid)}

    def find_templates(self, source_lambda):
        """The user's own lambda for each operator name, where one exists.

        A canvas item forwards what it does not understand to what it wraps, so
        the item over a `Mult` answers `LambdaName` with "Mult" as readily as
        the lambda does. Cloning the item would nest a canvas node inside a
        canvas node — the inner one positions itself in canvas space and draws
        somewhere else, leaving an empty frame behind. So skip anything that
        answers as a wrapper and take what it wraps.
        """
        found = {}
        for node in self.subtree(source_lambda):
            if self.snap.send_request(node, dex.CanvasNodeChild()) is not None:
                continue
            name = normalise(self.snap.send_request(node, dex.LambdaName()))
            if name in PRIMITIVE_SOURCE and name not in found:
                found[name] = node
        return found

    def constant(self, expr):
        """A literal node on the derivative's canvas."""
        uid = dex.NodeUid.mint()
        self.ws.insert_node_at_dyn(uid, as_number(expr.text))
        # The item, not the number: a bare value on a canvas is not something a
        # wire can reach.
        return self.place_at(uid, self.position(expr, CONST_SIZE))

    def operator(self, name, expr):
        """A fresh operator node; returns `(uid, [ports])`.

        Cloned from the user's own lambda when the source canvas has one, so
        the derivative computes with the same definition of `Mult` the original
        did. Only an operator the source never used is built from scratch.
        """
        template = self.templates.get(name)
        if template is not None:
            ids = self.plan_clone(template)
            self.ws.deep_clone_as(template, list(ids.items()))
            uid = ids[template]
            ports = [
                ids[port]
                for (_n, port, _s) in self.snap.send_request(
                    template, dex.DataflowInputs()
                )
            ]
            output = ids[self.snap.send_request(template, dex.DataflowOutput())]
        else:
            uid, args, output = dex.NodeUid.mint(), dex.NodeUid.mint(), dex.NodeUid.mint()
            ports = [dex.NodeUid.mint(), dex.NodeUid.mint()]
            self.ws.insert_node_at_dyn(
                uid,
                dex.Lambda.new_with(
                    self.ws, args, output, name.capitalize(), PRIMITIVE_SOURCE[name]
                ),
            )
            for port, label in zip(ports, ("a", "b")):
                arg = dex.NodeUid.mint()
                dex.LambdaArg.build_with(self.ws, arg, port, label)
                self.ws.submit_action(args, dex.AddArgAt(arg))

        self.place_at(uid, self.position(expr, OP_SIZE))
        return ports, output

    def prune(self, pins):
        """Take the cloned original apart, leaving only what is still pointed at.

        The clone exists so the product rule has `u` and `v` to reuse; folding
        removes most of those references, and what is left behind is scaffolding
        the reader has to look past. Parameter pins always stay — they are the
        lambda's own arguments, not part of the body.
        """
        for item in self.snap.send_request(self.source_canvas, dex.CanvasChildren()) or []:
            if item in pins or item in self.reached:
                continue
            inner = self.snap.send_request(item, dex.CanvasNodeChild())
            if inner is not None and inner in self.reached:
                continue
            self.ws.submit_action(
                self.canvas, dex.RemoveCanvasItem(self.clone[item])
            )

    def reachable(self, roots):
        """Every source uid the emitted derivative still depends on."""
        seen, queue = set(), list(roots)
        while queue:
            node = queue.pop()
            if node is None or node in seen:
                continue
            seen.add(node)
            # Upwards, so the item wrapping a kept node is kept too.
            queue.append(self.snap.owner_of(node))
            for (_n, _p, source) in (
                self.snap.send_request(node, dex.DataflowInputs()) or []
            ):
                queue.append(source)
        return seen

    def emit(self, expr):
        """Build `expr` on the derivative's canvas; returns what to wire to."""
        if isinstance(expr, Const):
            return self.constant(expr)
        if isinstance(expr, Var):
            # A pin is a value already: point straight at the clone's own pin.
            return self.clone[expr.source]

        if isinstance(expr, Ref):
            # An operator is *not* a value. `read` tags a term with the node
            # that computes it, so pointing a wire there would bind the node
            # itself rather than what it produces. Ask it what stands for its
            # value and take the clone of that.
            output = self.snap.send_request(expr.source, dex.DataflowOutput())
            if output is None:
                raise Unknown("a term with no output cannot be reused")
            return self.clone[output]

        operands = [self.emit(o) for o in expr.operands]
        ports, output = self.operator(expr.name, expr)
        for port, operand in zip(ports, operands):
            self.ws.submit_action(port, dex.SetConnection(operand))
        return output


def as_number(text):
    """A literal as the number it is, so it reaches scripts as one."""
    value = float(text)
    return int(value) if value.is_integer() else value


# ======================================================================
# The transform
# ======================================================================


def derivative(snap, ws, source_lambda, var):
    """Build d(`source_lambda`)/d(`var`) and return its root uid."""
    inputs = snap.send_request(source_lambda, dex.DataflowInputs()) or []
    names = [normalise(name) for (name, _port, _source) in inputs]
    wanted = normalise(var)
    if wanted not in names:
        raise Unknown(f"`{var}` is not a parameter of this lambda")
    index = names.index(wanted)

    pins = snap.send_request(source_lambda, dex.ParamPins()) or []
    body = snap.send_request(source_lambda, dex.LambdaBody())
    if body is None:
        raise Unknown("this lambda's output is not wired to anything")

    expression = read(snap, body, pins)
    derived = fold(differentiate(expression, index))

    builder = Builder(snap, ws, source_lambda, derived, layout(derived))
    result = builder.emit(derived)
    builder.prune(pins)

    # Point the copy's output at the derivative instead of the original body.
    ws.submit_action(
        snap.send_request(source_lambda, dex.OutputPin()) and builder.clone[
            snap.send_request(source_lambda, dex.OutputPin())
        ],
        dex.SetConnection(result),
    )
    ws.submit_action(
        builder.clone[snap.send_request(source_lambda, dex.LambdaNameNode())],
        dex.SetText(f"d/d{var}"),
    )
    return builder.root


def transform():
    return derivative(dex.snapshot, dex.ws, f, var)
