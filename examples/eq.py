"""A canvas lambda for one equation, written in rather than wired in.

The equation and its parameters are constants below: edit them, press Update,
and the transform returns a canvas lambda computing that expression. Use this
when you just want an equation to point `symbolic_derivative.py` at, without
first wiring up two text nodes to feed `gen_eq.py`.

Everything below the constants is `gen_eq.py`; see there for how the build
works.
"""

import ast

# Edit these two.
EQUATION = "a*x**2 + b*x + c"
PARAMS = "x a b c"

ADD, SUB, MULT, DIV, POW = "add", "sub", "mult", "div", "pow"

# The body of each operator lambda. `a` and `b` are the two parameters every
# operator takes, named to match `OPERANDS` below.
PRIMITIVE_SOURCE = {
    ADD: "def transform():\n    return a + b\n",
    SUB: "def transform():\n    return a - b\n",
    MULT: "def transform():\n    return a * b\n",
    DIV: "def transform():\n    return a / b\n",
    POW: "def transform():\n    return a ** b\n",
}
OPERANDS = ("a", "b")

BY_AST = {
    ast.Add: ADD,
    ast.Sub: SUB,
    ast.Mult: MULT,
    ast.Div: DIV,
    ast.Pow: POW,
}

# An operator is a `Lambda`, and a lambda's column needs room for its editor,
# its buttons and its output row. Laid out shorter than this the column runs out
# of height and stops before the output draws — which leaves it unrecorded, and
# a wire can only be drawn to something the halo recorded. Matches the size the
# sidebar builds a lambda at.
OP_SIZE = (420.0, 340.0)
CONST_SIZE = (60.0, 30.0)
COLUMN_W = 480.0
ROW_H = 380.0
# Each step towards the result drops as well as moving right, so a chain reads
# diagonally instead of as a flat row.
STEP_DOWN = 110.0


class Bad(Exception):
    """The equation asks for something this builder cannot express."""


# ======================================================================
# Parsing
# ======================================================================


class Node:
    """One term of the parsed equation."""


class Const(Node):
    def __init__(self, value):
        self.value = value


class Param(Node):
    def __init__(self, index):
        self.index = index


class Op(Node):
    def __init__(self, name, operands):
        self.name = name
        self.operands = operands


def parse(equation, params):
    """`equation` as a tree, with names resolved against `params`."""

    def convert(node):
        if isinstance(node, ast.BinOp):
            name = BY_AST.get(type(node.op))
            if name is None:
                raise Bad(f"`{type(node.op).__name__}` is not one of the operators")
            return Op(name, [convert(node.left), convert(node.right)])
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
            # `-u` is `0 - u`, so it needs no operator of its own.
            return Op(SUB, [Const(0), convert(node.operand)])
        if isinstance(node, ast.Constant) and isinstance(node.value, (int, float)):
            return Const(node.value)
        if isinstance(node, ast.Name):
            if node.id not in params:
                raise Bad(f"`{node.id}` is not one of the parameters")
            return Param(params.index(node.id))
        raise Bad(f"`{ast.dump(node)}` is not something this builds")

    return convert(ast.parse(equation.strip(), mode="eval").body)


# ======================================================================
# Layout
# ======================================================================


def assign_cells(node, depth, row, cells):
    """A `(depth, row)` for every node; returns the rows the subtree uses."""
    if not isinstance(node, Op):
        cells[id(node)] = (depth, row)
        return 1
    used = 0
    for operand in node.operands:
        used += assign_cells(operand, depth + 1, row + used, cells)
    cells[id(node)] = (depth, row + (used - 1) / 2)
    return used


# ======================================================================
# Building
# ======================================================================


class Builder:
    """Lays the equation out on a canvas lambda's inner surface."""

    def __init__(self, ws, canvas, pins, cells):
        self.ws = ws
        self.canvas = canvas
        self.pins = pins
        self.cells = cells
        self.right = max(depth for depth, _row in cells.values())

    def place(self, node, uid, size):
        """Put `uid` on the canvas; returns the item wrapping it.

        The item is what a wire can point at — the canvas draws its children as
        the addressable things, not what they wrap — and it stands for its
        child's value, so pointing at it reads the same.
        """
        depth, row = self.cells[id(node)]
        step = self.right - depth
        item = dex.CanvasNode.build(
            self.ws,
            uid,
            dex.Vector.new(step * COLUMN_W, row * ROW_H + step * STEP_DOWN),
            dex.Vector.new(size[0], size[1]),
        )
        self.ws.submit_action(self.canvas, dex.AdoptCanvasNode(item))
        return item

    def emit(self, node):
        """Build `node`; returns the uid a consumer should wire to."""
        if isinstance(node, Param):
            # A parameter is already on the canvas, as a pin.
            return self.pins[node.index]

        if isinstance(node, Const):
            uid = dex.NodeUid.mint()
            self.ws.insert_node_at_dyn(uid, node.value)
            # The item, not the number: a bare value on a canvas is not
            # something a wire can reach.
            return self.place(node, uid, CONST_SIZE)

        operands = [self.emit(operand) for operand in node.operands]

        uid, args, output = dex.NodeUid.mint(), dex.NodeUid.mint(), dex.NodeUid.mint()
        self.ws.insert_node_at_dyn(
            uid,
            dex.Lambda.new_with(
                self.ws,
                args,
                output,
                node.name.capitalize(),
                PRIMITIVE_SOURCE[node.name],
            ),
        )
        for label, operand in zip(OPERANDS, operands):
            arg, port = dex.NodeUid.mint(), dex.NodeUid.mint()
            dex.LambdaArg.build_with(self.ws, arg, port, label)
            self.ws.submit_action(args, dex.AddArgAt(arg))
            self.ws.submit_action(port, dex.SetConnection(operand))
        self.place(node, uid, OP_SIZE)
        return output


def build(ws, equation, params):
    """A canvas lambda over `params` computing `equation`; returns its uid."""
    tree = parse(equation, params)

    # Every id is minted up front: the queue does not drain until this returns,
    # so nothing built here could be looked up afterwards.
    root, args = dex.NodeUid.mint(), dex.NodeUid.mint()
    canvas, output_port = dex.NodeUid.mint(), dex.NodeUid.mint()
    pins = [dex.NodeUid.mint() for _ in params]

    ws.insert_node_at_dyn(
        root,
        dex.CanvasLambda.new_with(ws, args, canvas, output_port, equation),
    )
    for name in params:
        arg, port = dex.NodeUid.mint(), dex.NodeUid.mint()
        dex.LambdaArg.build_with(ws, arg, port, name)
        ws.submit_action(args, dex.AddArgAt(arg))

    # The pins mirror the parameters. Naming them here rather than waiting for
    # the lambda's own tick to mint them is what lets the body wire to them in
    # this same pass. Each starts pointing at nothing; the lambda's own tick
    # points it at whatever its argument is wired to.
    ws.submit_action(
        canvas, dex.SyncParamsAt([(pin, name, None) for pin, name in zip(pins, params)])
    )

    cells = {}
    assign_cells(tree, 0, 0, cells)
    result = Builder(ws, canvas, pins, cells).emit(tree)
    ws.submit_action(output_port, dex.SetConnection(result))
    return root


def transform():
    return build(dex.ws, EQUATION, PARAMS.split())
