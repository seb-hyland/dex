//! Gathering a lambda and everything feeding it into a canvas lambda of its own.
//!
//! A chain of lambdas wired across a desktop is a function that nobody has
//! written down: it has inputs (whatever its far end is wired to), a result
//! (whatever its far end is wired to), a result (whatever you were looking at),
//! and a body in between. Naming it is a matter of drawing a box around it,
//! which is what this does — the chain is copied onto a canvas lambda's
//! surface and the node you asked about becomes its result.
//!
//! What becomes an *argument* is only what feeds a tip: a lambda at the far end
//! of the chain, with nothing gathered behind it. A value wired into the middle
//! is not part of the function's shape — it is a coefficient, a constant, a
//! table someone dropped in — so it is copied onto the surface along with the
//! body and stays wired where it was. A chain of six operators fed by two
//! numbers takes two arguments, not six.
//!
//! The original is left exactly as it was. What comes back is a copy, so the
//! two can be compared, and so that reaching for this is never destructive.

use std::collections::{HashMap, HashSet};

use dex_core::prelude::*;

use crate::composites::button::Button;
use crate::composites::lambda::{
    AddArgAt, CanvasLambda, ComputeCanvas, ConnectionPort, DataflowInputs, LambdaArg, LambdaArgs,
    LambdaName, SetConnection, SyncParamsAt,
};
use crate::layouts::canvas::layout::{AdoptCanvasNode, Layer, PlaceOnCanvas};
use crate::layouts::canvas::nodes::{
    CanvasItemBounds, CanvasNode, CanvasNodeChild, CanvasNodeConstraints, SetLayout,
};
use crate::layouts::inspector::{menu_button, short_name};
use crate::layouts::vertical::VerticalLayout;
use crate::primitives::interaction::TakeClicked;
use crate::scripting::DataflowOutput;

/// How far below the parameter pins the gathered body starts, so the two do not
/// overlap on a surface whose pins sit along the top.
const PIN_CLEARANCE: f32 = 90.0;
/// Where a gathered lambda goes when it was never on a canvas and so has no
/// place of its own to keep.
const FALLBACK_SIZE: Vector = Vector { x: 420.0, y: 340.0 };
const FALLBACK_STEP: f32 = 380.0;
/// Where a carried value goes when it was never on a surface, and so has no
/// place of its own to keep: a row above the body it feeds.
const CARRIED_SIZE: Vector = Vector { x: 120.0, y: 36.0 };
const CARRIED_LIFT: f32 = 60.0;
/// The size the finished canvas lambda is placed at.
const GATHERED_SIZE: Vector = Vector { x: 320.0, y: 260.0 };
/// How deep the walk will follow wires before giving up. A graph this deep is a
/// mistake, and following it forever is a worse one.
const MAX_GATHERED: usize = 256;

/// What a surface knows about the nodes on it, gathered in one pass.
struct Surfaces {
    /// From the id a wire points at, to the lambda that produces it.
    producer_of: HashMap<NodeUid, NodeUid>,
    /// From a node, to the canvas item framing it.
    item_of: HashMap<NodeUid, NodeUid>,
}

/**
    Read every surface once, rather than once per edge followed.

    A canvas item forwards what it does not understand to what it frames, so the
    item over a lambda answers `DataflowOutput` exactly as the lambda does and
    would claim the same result. The wrappers are seen out by the one question
    only a wrapper answers.
*/
fn survey(ws: &Workspace) -> Surfaces {
    let mut producer_of = HashMap::new();
    let mut item_of = HashMap::new();
    for uid in ws.live_ids() {
        match ws.send_request(uid, CanvasNodeChild) {
            Some(child) => {
                item_of.insert(child, uid);
            }
            None => {
                if let Some(output) = ws.send_request(uid, DataflowOutput).flatten() {
                    producer_of.insert(output, uid);
                }
            }
        }
    }
    Surfaces {
        producer_of,
        item_of,
    }
}

/// `start`, and every lambda feeding it however far back, each named once.
fn upstream(ws: &Workspace, surfaces: &Surfaces, start: NodeUid) -> Vec<NodeUid> {
    let mut found = vec![start];
    let mut seen = HashSet::from([start]);
    let mut queue = vec![start];
    while let Some(current) = queue.pop() {
        if found.len() >= MAX_GATHERED {
            break;
        }
        for (_name, _port, target) in ws.send_request(current, DataflowInputs).unwrap_or_default() {
            let Some(producer) = target.and_then(|t| surfaces.producer_of.get(&t)).copied() else {
                continue;
            };
            if seen.insert(producer) {
                found.push(producer);
                queue.push(producer);
            }
        }
    }
    found
}

/**
    The gathered lambdas with nothing gathered behind them.

    The far end of the chain, and so the only place an argument comes from.
    Everything else has its inputs either wired to something else in the chain,
    which becomes an edge, or wired outside it, which comes along as a copy.
*/
fn tips(ws: &Workspace, surfaces: &Surfaces, gathered: &HashSet<NodeUid>) -> HashSet<NodeUid> {
    gathered
        .iter()
        .copied()
        .filter(|&lambda| {
            !ws.send_request(lambda, DataflowInputs)
                .unwrap_or_default()
                .into_iter()
                .any(|(_name, _port, target)| {
                    target
                        .and_then(|t| surfaces.producer_of.get(&t))
                        .is_some_and(|producer| gathered.contains(producer))
                })
        })
        .collect()
}

/// Everything `uid` owns, transitively, itself included.
fn subtree(ws: &Workspace, uid: NodeUid) -> Vec<NodeUid> {
    let mut found = Vec::new();
    let mut seen = HashSet::from([uid]);
    let mut queue = vec![uid];
    while let Some(current) = queue.pop() {
        found.push(current);
        if let Some(node) = ws.get_node(current) {
            node.owned_refs(&mut |child| {
                if child != NodeUid::nil() && seen.insert(child) {
                    queue.push(child);
                }
            });
        }
    }
    found
}

/**
    A fresh id for every node in `uid`'s subtree, chosen up front.

    A plain deep clone mints these itself and reports only the root, which is no
    help here: the ports that have to be rewired are all inside it.
*/
fn plan_clone(ws: &Workspace, uid: NodeUid) -> HashMap<NodeUid, NodeUid> {
    subtree(ws, uid)
        .into_iter()
        .map(|node| (node, NodeUid::mint()))
        .collect()
}

/**
    One outside value carried in with the chain rather than lifted out of it.

    What is wired into the middle of a chain is part of how the body works, not
    part of its shape, so a copy of it comes along and stays wired where it was.
*/
struct Carried {
    /// What the wire pointed at, which is what the copied port must point at.
    target: NodeUid,
    /// The canvas item framing it, so a copy keeps its place and its size. The
    /// target itself when nothing frames it.
    item: NodeUid,
    /// The copied ports that should end up pointing at the copy.
    ports: Vec<NodeUid>,
}

/// One loose end at the top of the gathered chain, which becomes an argument.
struct Input {
    name: String,
    /// What the original was wired to, outside everything gathered.
    source: Option<NodeUid>,
    /// The cloned ports that should end up pointing at this argument's pin.
    ports: Vec<NodeUid>,
}

/// A name not already taken, suffixed until it is free.
fn unique(name: &str, taken: &HashSet<String>) -> String {
    if !taken.contains(name) {
        return name.to_owned();
    }
    (2..)
        .map(|n| format!("{name}_{n}"))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or_else(|| name.to_owned())
}

/**
    Copy `source` and everything upstream of it onto a canvas lambda of its own.

    Returns the new lambda's id. Nothing is read back out of the workspace
    afterwards, so every id the wiring needs is chosen before anything is
    queued — the actions have not drained yet, and there would be nothing to
    look up.
*/
pub fn canvas_from_upstream(ws: &Workspace, source: NodeUid) -> NodeUid<CanvasLambda> {
    let handle = ws.action_handle();
    let surfaces = survey(ws);
    let chain = upstream(ws, &surfaces, source);
    let gathered: HashSet<NodeUid> = chain.iter().copied().collect();

    // Every gathered lambda copied, under ids chosen now so its ports can be
    // rewired in the same pass.
    let plans: HashMap<NodeUid, HashMap<NodeUid, NodeUid>> = chain
        .iter()
        .map(|&lambda| {
            let ids = plan_clone(ws, lambda);
            handle.deep_clone_as(lambda, ids.iter().map(|(a, b)| (*a, *b)).collect());
            (lambda, ids)
        })
        .collect();

    // The canvas lambda itself. Its three named parts are what the wiring
    // below has to address: somewhere to hang arguments, a surface to put the
    // body on, and the pin that says which node is the result.
    let uid = NodeUid::<CanvasLambda>::mint();
    let args = NodeUid::<LambdaArgs>::mint();
    let compute_canvas = NodeUid::<ComputeCanvas>::mint();
    let output_port = NodeUid::mint();
    let name = ws
        .send_request(source, LambdaName)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| {
            ws.get_node(source)
                .map(|node| {
                    short_name(&node.type_name(NodeContext {
                        id: source,
                        workspace: ws,
                    }))
                })
                .unwrap_or_else(|| "Lambda".to_owned())
        });
    handle.insert_node_at(
        uid,
        CanvasLambda::new_with(
            handle.clone(),
            args,
            compute_canvas,
            output_port,
            format!("{name} chain"),
        ),
    );

    // What becomes of each input is decided first, because the values carried
    // in with the body are part of the arrangement everything is laid out by.
    let tips = tips(ws, &surfaces, &gathered);
    let (inputs, carried) = rewire(ws, &handle, &surfaces, &chain, &gathered, &tips, &plans);
    let origin = arrangement_origin(ws, &surfaces, &chain, &carried);

    place_bodies(
        ws,
        &handle,
        &surfaces,
        &chain,
        &plans,
        compute_canvas,
        origin,
    );
    carry_in(ws, &handle, &carried, compute_canvas, origin);
    hang_arguments(&handle, args, compute_canvas, inputs);

    // The result is whatever the node you asked about computes.
    let result = ws
        .send_request(source, DataflowOutput)
        .flatten()
        .and_then(|output| plans[&source].get(&output).copied());
    handle.submit_action(
        output_port.cast::<ConnectionPort>(),
        "Took the gathered result",
        SetConnection { target: result },
    );

    handle.submit_action(
        ws.root(),
        "Placed the gathered chain",
        PlaceOnCanvas {
            node: uid.erase(),
            size: GATHERED_SIZE,
        },
    );
    uid
}

/**
    The top-left of everything gathered, in the old surface's coordinates.

    Both the bodies and the values carried in with them, so that shifting the
    whole arrangement by it keeps them in the same places relative to each
    other — a constant sitting to the left of the operator it feeds stays there.
*/
fn arrangement_origin(
    ws: &Workspace,
    surfaces: &Surfaces,
    chain: &[NodeUid],
    carried: &[Carried],
) -> Vector {
    chain
        .iter()
        .filter_map(|lambda| surfaces.item_of.get(lambda).copied())
        .chain(carried.iter().map(|value| value.item))
        .filter_map(|item| ws.send_request(item, CanvasNodeConstraints))
        .fold(None::<Vector>, |acc, layout| match acc {
            Some(min) => Some(Vector {
                x: min.x.min(layout.pos.x),
                y: min.y.min(layout.pos.y),
            }),
            None => Some(layout.pos),
        })
        .unwrap_or(Vector::ZERO)
}

/**
    Put each copy on the new surface where its original sat on the old one.

    Relative positions, shifted so the whole chain sits under the parameter
    pins. Laying it out afresh would be tidier and would also throw away the
    arrangement someone made on purpose; a chain read left to right stays read
    left to right.
*/
fn place_bodies(
    ws: &Workspace,
    handle: &WorkspaceActionHandle,
    surfaces: &Surfaces,
    chain: &[NodeUid],
    plans: &HashMap<NodeUid, HashMap<NodeUid, NodeUid>>,
    compute_canvas: NodeUid<ComputeCanvas>,
    origin: Vector,
) {
    let placed: Vec<(NodeUid, Option<(Vector, Vector)>)> = chain
        .iter()
        .map(|&lambda| {
            let layout = surfaces
                .item_of
                .get(&lambda)
                .and_then(|&item| ws.send_request(item, CanvasNodeConstraints))
                .map(|layout| (layout.pos, layout.size));
            (lambda, layout)
        })
        .collect();

    // A lambda that was never on a canvas has no place to keep, so it is given
    // one: a row of its own, past the ones that do.
    let mut loose = 0.0;
    for (lambda, layout) in placed {
        let (pos, size) = match layout {
            Some((pos, size)) => (
                Vector {
                    x: pos.x - origin.x,
                    y: pos.y - origin.y + PIN_CLEARANCE,
                },
                size,
            ),
            None => {
                let pos = Vector {
                    x: loose,
                    y: PIN_CLEARANCE,
                };
                loose += FALLBACK_STEP;
                (pos, FALLBACK_SIZE)
            }
        };
        let copy = plans[&lambda][&lambda];
        let item = CanvasNode::build(handle.clone(), copy, pos, size);
        handle.submit_action(
            compute_canvas.erase(),
            "Placed a gathered lambda",
            AdoptCanvasNode {
                node: item.erase(),
                layer: Layer::Midground,
            },
        );
    }
}

/**
    Decide what becomes of every input, and wire the ones that stay inside.

    Three fates. An input wired to something else that was gathered is an edge
    within the new surface, and is wired here. An input on a *tip* — a lambda at
    the far end, with nothing gathered behind it — is a loose end, and becomes an
    argument of the canvas as a whole. Anything else is a value wired into the
    middle of the chain, which is part of how the body works rather than part of
    its shape: a copy of it comes along and the port stays pointing at it.

    That last rule is what keeps the signature honest. Without it a chain of six
    operators fed by two numbers came out asking for six arguments, most of them
    naming a constant that only ever had one value.
*/
fn rewire(
    ws: &Workspace,
    handle: &WorkspaceActionHandle,
    surfaces: &Surfaces,
    chain: &[NodeUid],
    gathered: &HashSet<NodeUid>,
    tips: &HashSet<NodeUid>,
    plans: &HashMap<NodeUid, HashMap<NodeUid, NodeUid>>,
) -> (Vec<Input>, Vec<Carried>) {
    let mut inputs: Vec<Input> = Vec::new();
    let mut carried: Vec<Carried> = Vec::new();
    let mut taken: HashSet<String> = HashSet::new();
    for &lambda in chain {
        let ids = &plans[&lambda];
        for (name, port, target) in ws.send_request(lambda, DataflowInputs).unwrap_or_default() {
            let Some(&copied_port) = ids.get(&port) else {
                continue;
            };
            let producer = target
                .and_then(|t| surfaces.producer_of.get(&t))
                .copied()
                .filter(|producer| gathered.contains(producer));

            if let Some(producer) = producer {
                let inside = ws
                    .send_request(producer, DataflowOutput)
                    .flatten()
                    .and_then(|output| plans[&producer].get(&output).copied());
                handle.submit_action(
                    copied_port.cast::<ConnectionPort>(),
                    "Rewired a gathered argument",
                    SetConnection { target: inside },
                );
                continue;
            }

            if !tips.contains(&lambda) {
                // Wired into the middle from outside: the value comes along.
                // Nothing on the port at all is left as it is — there is no
                // value to bring, and the chain never had one either.
                let Some(target) = target else { continue };
                match carried.iter_mut().find(|c| c.target == target) {
                    // One node feeding two ports is copied once.
                    Some(existing) => existing.ports.push(copied_port),
                    None => carried.push(Carried {
                        target,
                        item: framing(ws, surfaces, target),
                        ports: vec![copied_port],
                    }),
                }
                continue;
            }

            // Two arguments fed by one node outside are one argument here: the
            // chain takes that value once and uses it twice.
            if let Some(existing) =
                target.and_then(|t| inputs.iter_mut().find(|input| input.source == Some(t)))
            {
                existing.ports.push(copied_port);
                continue;
            }
            let name = unique(&name, &taken);
            taken.insert(name.clone());
            inputs.push(Input {
                name,
                source: target,
                ports: vec![copied_port],
            });
        }
    }
    (inputs, carried)
}

/**
    The canvas item framing `target`, so a copy of it keeps its place and size.

    `target` itself when it is already an item, and when nothing frames it at
    all — a value that was never on a surface has no place to keep, and is given
    one when it lands.
*/
fn framing(ws: &Workspace, surfaces: &Surfaces, target: NodeUid) -> NodeUid {
    if ws.send_request(target, CanvasItemBounds).is_some() {
        return target;
    }
    surfaces.item_of.get(&target).copied().unwrap_or(target)
}

/**
    Copy each carried value onto the new surface and point its ports at it.

    The *item* is copied when there is one, so the value arrives where it sat
    and at the size it was; a value that was never on a surface is copied bare
    and framed on the way in.
*/
fn carry_in(
    ws: &Workspace,
    handle: &WorkspaceActionHandle,
    carried: &[Carried],
    compute_canvas: NodeUid<ComputeCanvas>,
    origin: Vector,
) {
    let mut loose = 0.0;
    for value in carried {
        let ids = plan_clone(ws, value.item);
        handle.deep_clone_as(value.item, ids.iter().map(|(a, b)| (*a, *b)).collect());
        let copied_item = ids[&value.item];

        match ws.send_request(value.item, CanvasNodeConstraints) {
            Some(layout) => {
                // The copy carries the original's layout, which is in the old
                // surface's coordinates; the whole arrangement is shifted.
                handle.submit_action(
                    copied_item.cast::<CanvasNode>(),
                    "Kept a carried value's place",
                    SetLayout {
                        canvas_pos: Vector {
                            x: layout.pos.x - origin.x,
                            y: layout.pos.y - origin.y + PIN_CLEARANCE,
                        },
                        size: layout.size,
                    },
                );
                handle.submit_action(
                    compute_canvas.erase(),
                    "Carried a value in",
                    AdoptCanvasNode {
                        node: copied_item,
                        layer: Layer::Midground,
                    },
                );
            }
            None => {
                let item = CanvasNode::build(
                    handle.clone(),
                    copied_item,
                    Vector {
                        x: loose,
                        y: PIN_CLEARANCE - CARRIED_LIFT,
                    },
                    CARRIED_SIZE,
                );
                loose += CARRIED_SIZE.x + FALLBACK_STEP * 0.1;
                handle.submit_action(
                    compute_canvas.erase(),
                    "Carried a value in",
                    AdoptCanvasNode {
                        node: item.erase(),
                        layer: Layer::Midground,
                    },
                );
            }
        }

        // The ports point at the copy of what the wire pointed at, which is
        // not the item when the wire reached past it to the value inside.
        let inside = ids.get(&value.target).copied().unwrap_or(copied_item);
        for &port in &value.ports {
            handle.submit_action(
                port.cast::<ConnectionPort>(),
                "Fed a carried value in",
                SetConnection {
                    target: Some(inside),
                },
            );
        }
    }
}

/// Give the new lambda one argument per loose end, and a pin to match.
fn hang_arguments(
    handle: &WorkspaceActionHandle,
    args: NodeUid<LambdaArgs>,
    compute_canvas: NodeUid<ComputeCanvas>,
    inputs: Vec<Input>,
) {
    let mut pins = Vec::new();
    for input in inputs {
        let arg = NodeUid::<LambdaArg>::mint();
        let arg_port = NodeUid::mint();
        LambdaArg::build_with(handle.clone(), arg, arg_port, input.name.clone());
        handle.submit_action(
            args,
            "Hung a gathered argument",
            AddArgAt { arg: arg.erase() },
        );
        // The argument keeps pointing at whatever the original was fed by, so
        // the copy computes the same thing the moment it lands.
        handle.submit_action(
            arg_port.cast::<ConnectionPort>(),
            "Kept a gathered argument's source",
            SetConnection {
                target: input.source,
            },
        );

        // The pin is named now rather than left to the next sync, which would
        // mint it and leave nothing for the body to be wired to.
        let pin = NodeUid::mint();
        pins.push((pin, input.name, input.source));
        for port in input.ports {
            handle.submit_action(
                port.cast::<ConnectionPort>(),
                "Fed a gathered argument in",
                SetConnection { target: Some(pin) },
            );
        }
    }
    handle.submit_action(
        compute_canvas,
        "Named the gathered parameters",
        SyncParamsAt { entries: pins },
    );
}

/// What a lambda adds to the inspector: the one thing you can do to a whole
/// chain of them at once.
#[utils::portable]
pub struct LambdaCommands {
    /// The lambda these act on.
    #[uid_ref]
    target: NodeUid,
    gather_button: NodeUid<Button>,
    column: NodeUid<VerticalLayout>,
}

impl LambdaCommands {
    /// Build the commands for `target`, to sit in its inspector.
    pub fn build(ws: WorkspaceActionHandle, target: NodeUid) -> NodeUid<LambdaCommands> {
        let gather_button = menu_button(ws.clone(), "Canvas from upstream");
        let column = VerticalLayout::build(ws.clone(), vec![gather_button.erase()], 2.0);
        ws.insert_node(Self {
            target,
            gather_button,
            column,
        })
    }
}

#[utils::dynamic_node(skip)]
impl Node for LambdaCommands {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "Lambda Commands".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;
        let drawn = ctx.draw_workspace_node(self.column.erase(), constraints);

        let ws = ctx.node.workspace;
        // Taken, so the command fires once: this row stops being drawn the
        // moment the menu closes, and a plain read would repeat the last click.
        if ws
            .send_request(self.gather_button.erase(), TakeClicked)
            .unwrap_or(false)
        {
            canvas_from_upstream(ws, self.target);
        }

        drawn.unwrap_or(DrawResult::Complete { region: None })
    }

    fn on_delete(&self, ctx: NodeContext) {
        ctx.workspace.delete_node(self.column.erase());
        ctx.workspace.delete_node(self.gather_button.erase());
    }
}

defhandlers! { LambdaCommands {} }
