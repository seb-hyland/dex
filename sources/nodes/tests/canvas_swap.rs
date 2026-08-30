//! Swapping one canvas item for another in place — how a line or a circle
//! becomes a path that can be shaped point by point.

use dex_core::prelude::*;
use dex_nodes::{
    layouts::canvas::{
        layout::{AddCanvasItem, Canvas, CanvasChildren, SwapCanvasItem},
        nodes::{
            CanvasItemBounds, CanvasNodeChild,
            editors::{PathAnchorOrigin, PathEditable},
        },
    },
    primitives::{
        nothing::Nothing,
        shapes::{Anchor, Circle, GetAnchors, GetRadius, IsPathClosed, IsPathFilled, Path},
    },
};

/// An empty workspace with a throwaway root, drained and ready.
fn workspace() -> Workspace {
    let mut ws = Workspace::new_empty();
    let root = ws.insert_node_now(Nothing);
    ws.set_root(root.erase());
    ws
}

#[test]
fn a_line_swaps_for_a_path_it_can_be_shaped_from() {
    let mut ws = workspace();
    let canvas = Canvas::build(ws.action_handle());
    ws.process_pending();

    let points = vec![Vector::new(0.0, 0.0), Vector::new(140.0, 60.0)];
    ws.submit_action(canvas, "line", AddCanvasItem {
        child: Arc::new(Path::polyline(points.clone(), Stroke::new(2.5, Color::BLACK))),
        size: Vector { x: 140.0, y: 60.0 },
    });
    ws.process_pending();

    let children = ws.send_request(canvas, CanvasChildren).unwrap_or_default();
    assert_eq!(children.len(), 1, "the line is the canvas's one item");
    let line_item = children[0];
    let line_child = ws
        .send_request(line_item, CanvasNodeChild)
        .expect("the line's editor wraps the path");
    let anchors: Vec<Anchor> = ws
        .send_request(line_child, GetAnchors)
        .expect("the wrapped node is a path");
    let pos = ws
        .send_request(line_item, PathAnchorOrigin)
        .expect("the editor reports where its anchors sit");

    // Convert: the same outline with a point added on the middle of it.
    let mut split = anchors.clone();
    split.insert(1, Anchor::corner(Vector::new(70.0, 30.0)));
    ws.submit_action(canvas, "convert", SwapCanvasItem {
        old: line_item,
        child: Arc::new(Path::open_through(split, Stroke::new(2.5, Color::BLACK))),
        pos,
        size: Vector { x: 140.0, y: 60.0 },
    });
    ws.process_pending();

    let children = ws.send_request(canvas, CanvasChildren).unwrap_or_default();
    assert_eq!(children.len(), 1, "the replacement took the line's slot");
    let poly_item = children[0];
    assert_ne!(poly_item, line_item, "it is a new item, not the old one");

    assert!(
        ws.get_node(line_item).is_none(),
        "the line's editor is gone from the registry"
    );
    assert!(
        ws.get_node(line_child).is_none(),
        "along with the path it wrapped"
    );

    let poly_child = ws
        .send_request(poly_item, CanvasNodeChild)
        .expect("the polygon's editor wraps a path");
    let poly_anchors: Vec<Anchor> = ws
        .send_request(poly_child, GetAnchors)
        .expect("the replacement is a path");
    let points = |v: &[Anchor]| v.iter().map(|a| (a.pos.x, a.pos.y)).collect::<Vec<_>>();
    assert_eq!(
        points(&poly_anchors),
        vec![(0.0, 0.0), (70.0, 30.0), (140.0, 60.0)],
        "the line's own ends are kept, with a third point on the line between"
    );
    // The added point is on the line, so nothing moved on screen.
    let (a, m, b) = (poly_anchors[0].pos, poly_anchors[1].pos, poly_anchors[2].pos);
    let cross = (b.x - a.x) * (m.y - a.y) - (b.y - a.y) * (m.x - a.x);
    assert!(cross.abs() < 1e-3, "the new point is collinear with the ends");
    assert_eq!(
        ws.send_request(poly_child, IsPathClosed),
        Some(false),
        "closing it is left to the inspector"
    );
    assert_eq!(ws.send_request(poly_child, IsPathFilled), Some(false));
    assert_eq!(
        ws.send_request(poly_item, PathEditable),
        Some(true),
        "and its points start live, so the new one can be dragged out"
    );
    let poly_pos = ws
        .send_request(poly_item, PathAnchorOrigin)
        .expect("the replacement editor reports its origin");
    assert_eq!(
        (poly_pos.x, poly_pos.y),
        (pos.x, pos.y),
        "and sits exactly where the line did"
    );
    assert!(
        ws.send_request(poly_item, CanvasItemBounds).is_some(),
        "the replacement answers the canvas-item protocol"
    );
}

/// A circle converts by being traced as a path: four smooth anchors on exactly
/// the circle it came from, so nothing moves and every point becomes editable.
#[test]
fn a_circle_swaps_for_a_path_that_traces_it() {
    let mut ws = workspace();
    let canvas = Canvas::build(ws.action_handle());
    ws.process_pending();

    let radius = 40.0;
    ws.submit_action(canvas, "circle", AddCanvasItem {
        child: Arc::new(Circle::new(radius, Color::rgb(120, 170, 220))),
        size: Vector::splat(radius * 2.0),
    });
    ws.process_pending();

    let circle_item = *ws
        .send_request(canvas, CanvasChildren)
        .unwrap_or_default()
        .first()
        .expect("the circle is the canvas's one item");
    let circle_child = ws
        .send_request(circle_item, CanvasNodeChild)
        .expect("the circle's editor wraps it");
    assert_eq!(ws.send_request(circle_child, GetRadius), Some(radius));

    let bounds = ws
        .send_request(circle_item, CanvasItemBounds)
        .expect("the circle answers the canvas-item protocol");
    let center = (
        (bounds.min.x + bounds.max.x) * 0.5,
        (bounds.min.y + bounds.max.y) * 0.5,
    );

    ws.submit_action(canvas, "convert", SwapCanvasItem {
        old: circle_item,
        child: Arc::new(Path::circle(
            Vector::splat(radius),
            radius,
            Path::default_fill(),
            Stroke::NONE,
        )),
        pos: bounds.min.to_vector(),
        size: bounds.size(),
    });
    ws.process_pending();

    let children = ws.send_request(canvas, CanvasChildren).unwrap_or_default();
    assert_eq!(children.len(), 1, "the path took the circle's slot");
    let path_item = children[0];
    assert!(
        ws.get_node(circle_item).is_none() && ws.get_node(circle_child).is_none(),
        "the circle and its editor are gone from the registry"
    );

    let path_child = ws
        .send_request(path_item, CanvasNodeChild)
        .expect("the path's editor wraps it");
    let anchors: Vec<Anchor> = ws
        .send_request(path_child, GetAnchors)
        .expect("the replacement is a path");
    assert_eq!(anchors.len(), 4, "one anchor per quarter arc");
    assert_eq!(ws.send_request(path_child, IsPathClosed), Some(true));
    assert_eq!(ws.send_request(path_child, IsPathFilled), Some(true));

    // Every anchor sits on the circle it was traced from, in canvas space.
    let origin = ws
        .send_request(path_item, PathAnchorOrigin)
        .expect("the editor reports its anchor origin");
    for a in &anchors {
        assert!(a.in_handle.is_some() && a.out_handle.is_some(), "smooth");
        let (x, y) = (origin.x + a.pos.x, origin.y + a.pos.y);
        let d = ((x - center.0).powi(2) + (y - center.1).powi(2)).sqrt();
        assert!(
            (d - radius).abs() < 1e-3,
            "anchor is {d} from the circle's centre, not {radius}"
        );
    }
}
