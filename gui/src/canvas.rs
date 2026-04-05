use crate::prelude::*;
use crate::{
    node::{
        DrawContext, Node, NodeDynamics, NodeVariant, dataframe::DataframePayload,
        transform::TransformArgPayload, view::Window,
    },
    registry::{Registry, RegistryItem, RegistryItemInner},
};

use std::path::PathBuf;

use eframe::{
    egui::{Color32, Id, Response, Sense, Shape, Stroke, StrokeKind, UiBuilder},
    emath::RectTransform,
    epaint::CircleShape,
};
use petgraph::{Directed, stable_graph::StableGraph, visit::EdgeRef};

pub type NodeIdx = petgraph::graph::NodeIndex<u32>;
pub type CanvasGraph = StableGraph<Node, (), Directed, u32>;

pub struct Canvas {
    pub graph: CanvasGraph,
    pub indices_by_depth: Vec<NodeIdx>,
    pub view_state: ViewState,
    pub placing_node: Option<NodeIdx>,
    pub connecting_nodes: NodeConnectionState,
}

pub struct ViewState {
    draw_surface: Rect,
    offset: Vec2,
    scale: f32,
    transform: RectTransform,
}

#[derive(Copy, Clone, PartialEq)]
pub enum NodeConnectionState {
    None,
    Searching,
    One(NodeIdx),
}

pub enum CanvasCommand {
    AddTransformArg { origin: NodeIdx },
    MoveNode { idx: NodeIdx, delta: Vec2 },
    AddEdge { start: NodeIdx, end: NodeIdx },
}

impl CanvasCommand {
    fn exe(self, canvas: &mut Canvas, dragged_node: &mut Option<NodeIdx>) {
        match self {
            CanvasCommand::AddTransformArg { origin } => {
                let new_idx =
                    canvas.add_node_noplacing(NodeVariant::TransformArg(TransformArgPayload));
                if let NodeVariant::Transform(ref mut t) =
                    canvas.graph.node_weight_mut(origin).unwrap().variant
                {
                    t.args.last_mut().unwrap().node = Some(new_idx);
                } else {
                    unreachable!("AddTransformArg called by non-transform node");
                }
            }
            CanvasCommand::MoveNode { idx, delta } => {
                *dragged_node = Some(idx);
                canvas.graph.node_weight_mut(idx).unwrap().location += delta;
            }
            CanvasCommand::AddEdge { start, end } => {
                canvas.graph.add_edge(start, end, ());
            }
        }
    }
}

impl Canvas {
    pub fn draw(&mut self, ui: &mut Ui, registry: &mut Registry) {
        let (response, painter) =
            ui.allocate_painter(ui.available_size_before_wrap(), Sense::drag());
        let canvas_rect = response.rect;
        let theme: Theme = ui.ctx().into();
        self.view_state.update_surface(canvas_rect);

        self.draw_background(canvas_rect, &painter, &theme);

        let mut command_queue = Vec::new();
        'node_loop: for cur_idx in self.indices_by_depth.iter().copied() {
            for outgoing_edge in self.graph.edges(cur_idx) {
                let source_node = self.graph.node_weight(cur_idx).unwrap();
                let source_location = source_node.location;

                let target_node = self.graph.node_weight(outgoing_edge.target()).unwrap();
                let target_location = target_node.location;

                let source_screen = self.view_state.world_to_screen(source_location);
                let target_screen = self.view_state.world_to_screen(target_location);

                painter.line_segment([source_screen, target_screen], theme.border);
            }

            let node = self.graph.node_weight_mut(cur_idx).unwrap();
            let node_id = Id::new("canvas_node").with(cur_idx);

            if matches!(node.variant, NodeVariant::TransformArg(_)) {
                continue 'node_loop;
            }

            let node_location = self.view_state.world_to_screen(node.location);
            let mut draw_context = DrawContext {
                index: cur_idx,
                id: node_id,
                screen_location: node_location,
                command_queue: &mut command_queue,
                view_state: &self.view_state,
                registry,
                ui,
                theme: &theme,
                noninteractive: false,
            };

            // At least part of the node is visible
            let node_rect = node.variant.rect(&mut draw_context);
            if (canvas_rect.contains(node_location) || canvas_rect.intersects(node_rect)) {
                let is_placing_node = matches!(self.placing_node, Some(idx) if idx == cur_idx);
                // If we are currently adding a new node and it is this one
                if is_placing_node
                    || matches!(
                        self.connecting_nodes,
                        NodeConnectionState::Searching | NodeConnectionState::One(_)
                    )
                {
                    draw_context.noninteractive = true;
                }

                // Draw yourself, I COMMAND you
                node.variant.draw(&mut draw_context);

                if draw_context.noninteractive {
                    // Steal interaction and sense placement
                    let resp = draw_context.ui.interact(
                        node_rect,
                        node_id.with("noninteractive_steal"),
                        Sense::click(),
                    );

                    if is_placing_node {
                        if let Some(cursor_screen_pos) =
                            draw_context.ui.ctx().input(|i| i.pointer.latest_pos())
                        {
                            // Sync its location with the pointer
                            let cursor_world_pos =
                                self.view_state.screen_to_world(cursor_screen_pos);
                            node.location = cursor_world_pos;
                        }
                    } else if resp.clicked() {
                        // Looking for edge
                        match self.connecting_nodes {
                            NodeConnectionState::Searching => {
                                self.connecting_nodes = NodeConnectionState::One(cur_idx);
                            }
                            NodeConnectionState::One(origin_idx) => {
                                draw_context.command_queue.push(CanvasCommand::AddEdge {
                                    start: origin_idx,
                                    end: cur_idx,
                                });
                                self.connecting_nodes = NodeConnectionState::None;
                            }
                            _ => unreachable!(),
                        }
                    }
                }
            }
        }

        if self.placing_node.is_some() {
            cursor_icon!(ui, Copy);
            if ui
                .interact(canvas_rect, Id::new("canvas_placement"), Sense::click())
                .clicked()
            {
                self.placing_node = None;
            }
        }
        let mut dragged_node = None;
        for command in command_queue {
            command.exe(self, &mut dragged_node);
        }

        // Move dragged node to front
        if let Some(dragged_idx) = dragged_node {
            let pos = self
                .indices_by_depth
                .iter()
                .position(|&idx| idx == dragged_idx)
                .unwrap();
            self.indices_by_depth.remove(pos);
            self.indices_by_depth.push(dragged_idx);
        }

        match DrawInteraction::from(response) {
            DrawInteraction::Hovered => cursor_icon!(ui, PointingHand),
            DrawInteraction::Dragged(drag_delta) => {
                cursor_icon!(ui, Grabbing);
                self.view_state
                    .update_offset(-drag_delta / self.view_state.scale());
            }
            _ => {}
        }
    }

    fn draw_background(&mut self, canvas_rect: Rect, painter: &Painter, theme: &Theme) {
        // If we are at an offset of 13, the first point should be 7 to the right.
        let offset = self.view_state.offset;
        let point_spacing = 30.0 * self.view_state.scale;

        let shift_x = offset.x % point_spacing;
        let shift_y = offset.y % point_spacing;
        let start_x = canvas_rect.left() - shift_x;
        let start_y = canvas_rect.top() - shift_y;

        let mut point_pos = egui::pos2(start_x, start_y);

        while point_pos.x <= canvas_rect.right() + point_spacing {
            while point_pos.y <= canvas_rect.bottom() + point_spacing {
                painter.add(Shape::Circle(CircleShape::filled(
                    point_pos,
                    1.5 * self.view_state.scale,
                    theme.faint_background,
                )));
                point_pos.y += point_spacing;
            }
            point_pos.y = start_y;
            point_pos.x += point_spacing;
        }
    }

    pub fn add_dataframe(&mut self, registry: &mut Registry, path: PathBuf, df: RecordBatch) {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();

        let data_ref = registry.insert(RegistryItem {
            backing_file: Some(path.clone()),
            inner: RegistryItemInner::Dataframe {
                table_name: name,
                data: df,
            },
        });

        let node = NodeVariant::Dataframe(DataframePayload {
            data_ref,
            view: Window::default(),
        });
        self.add_node(node);
    }

    pub fn add_node(&mut self, node: NodeVariant) {
        let idx = self.add_node_noplacing(node);
        self.placing_node = Some(idx);
    }

    pub fn add_node_noplacing(&mut self, node: NodeVariant) -> NodeIdx {
        let idx = self.graph.add_node(Node {
            location: Pos2::ZERO,
            variant: node,
        });
        self.indices_by_depth.push(idx);
        idx
    }
}

impl ViewState {
    pub fn new(draw_surface: Rect) -> Self {
        let offset = Vec2::ZERO;
        let scale = 1.0;
        let transform = Self::_make_transform(offset, scale, draw_surface);
        Self {
            draw_surface,
            offset,
            scale,
            transform,
        }
    }

    #[inline(always)]
    fn _make_transform(offset: Vec2, scale: f32, draw_surface: Rect) -> RectTransform {
        let world_rect = Rect::from_center_size(offset.to_pos2(), draw_surface.size() / scale);
        RectTransform::from_to(world_rect, draw_surface)
    }

    #[inline(always)]
    fn update_transform(&mut self) {
        self.transform = Self::_make_transform(self.offset, self.scale, self.draw_surface);
    }

    #[inline(always)]
    pub fn update_surface(&mut self, draw_surface: Rect) {
        self.draw_surface = draw_surface;
        self.update_transform();
    }

    #[inline(always)]
    pub fn offset(&self) -> Vec2 {
        self.offset
    }

    #[inline(always)]
    pub fn update_offset(&mut self, delta: Vec2) {
        self.offset += delta;
        self.update_transform();
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    #[inline(always)]
    pub fn update_scale(&mut self, zoom_factor: f32) {
        self.scale *= zoom_factor;
        self.update_transform();
    }

    #[inline(always)]
    pub fn world_to_screen(&self, pos: Pos2) -> Pos2 {
        self.transform.transform_pos(pos)
    }

    #[inline(always)]
    pub fn screen_to_world(&self, pos: Pos2) -> Pos2 {
        self.transform.inverse().transform_pos(pos)
    }
}

pub trait InteractExtension {
    fn interact_visible(&mut self, rect: Rect, id: Id, sense: Sense) -> Response;
}

impl InteractExtension for Ui {
    fn interact_visible(&mut self, rect: Rect, id: Id, sense: Sense) -> Response {
        let resp = self.interact(rect, id, sense);
        let interaction_str = match DrawInteraction::from(resp.clone()) {
            DrawInteraction::None => "",
            DrawInteraction::Clicked => "Clicked",
            DrawInteraction::Hovered => "Hovered",
            DrawInteraction::Dragged(_) => "Dragged",
        };
        self.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
            ui.painter().rect(
                rect,
                0.0,
                Color32::GREEN.gamma_multiply(0.3),
                Stroke::NONE,
                StrokeKind::Middle,
            );
            ui.label(interaction_str);
        });
        resp
    }
}
