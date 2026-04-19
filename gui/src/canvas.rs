use crate::prelude::*;
use crate::{
    actions::DoActionContext,
    node::{
        DrawContext, LayoutContext, Node, NodeDynamics, NodeVariant, dataframe::DataframePayload,
    },
    registry::{Registry, RegistryItem, RegistryItemInner},
    theme::Theme,
};

use std::ops::Deref;
use std::path::PathBuf;

use eframe::egui::Stroke;
use eframe::{
    egui::{Id, Sense, Shape},
    emath::RectTransform,
    epaint::CircleShape,
};
use petgraph::stable_graph::{EdgeReference, Edges};
use petgraph::{Directed, stable_graph::StableGraph, visit::EdgeRef};
use serde::{Deserialize, Serialize};

pub type NodeIdx = petgraph::graph::NodeIndex<u32>;

#[derive(Clone)]
pub struct Canvas {
    graph: CanvasGraph,
    indices_by_depth: Transient<Vec<NodeIdx>>,
    view_state: Rigid<ViewState>,
    background_visible: bool,
    placing_node: Rigid<Option<NodeIdx>>,
    connecting_nodes: Rigid<NodeConnectionState>,
    cached_draw_surface: Transient<Rect>,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            graph: Default::default(),
            indices_by_depth: Default::default(),
            view_state: Default::default(),
            background_visible: Default::default(),
            placing_node: Default::default(),
            connecting_nodes: Default::default(),
            cached_draw_surface: Transient::from(Rect::ZERO),
        }
    }
}

impl Deref for Canvas {
    type Target = CanvasGraph;

    fn deref(&self) -> &Self::Target {
        &self.graph
    }
}

impl Canvas {
    pub fn add_node(&mut self, node: NodeVariant) -> NodeIdx {
        self.add_node_seeded(|_| node)
    }

    pub fn add_node_seeded(&mut self, constructor: impl FnOnce(u32) -> NodeVariant) -> NodeIdx {
        let idx = self.add_node_noplacing_seeded(constructor);
        self.placing_node.set(Some(idx));
        idx
    }

    pub fn add_node_noplacing(&mut self, node: NodeVariant) -> NodeIdx {
        self.add_node_noplacing_seeded(|_| node)
    }

    pub fn add_node_noplacing_seeded(
        &mut self,
        constructor: impl FnOnce(u32) -> NodeVariant,
    ) -> NodeIdx {
        let idx = {
            let id = self.graph.cur_count;
            let variant = constructor(id);

            self.graph.cur_count += 1;
            self.graph.inner.add_node(Node {
                id: Id::new("canvas_node").with(id),
                location: Pos2::ZERO,
                variant,
            })
        };

        self.indices_by_depth.modify(|indices| indices.push(idx));
        idx
    }

    pub fn get_node_mut(&mut self, idx: NodeIdx) -> &mut Node {
        self.graph.inner.node_weight_mut(idx).unwrap()
    }

    pub fn add_edge(&mut self, origin: NodeIdx, target: NodeIdx) {
        self.graph.inner.add_edge(origin, target, ());
    }

    pub fn can_connect_nodes(&self) -> bool {
        self.connecting_nodes.val() == NodeConnectionState::None
    }

    pub fn start_node_connection_search(&self) {
        self.connecting_nodes.set(NodeConnectionState::Searching);
    }

    pub fn reset_view(&self) {
        self.view_state.modify(|state| state.reset_offset());
    }

    pub fn background_visible(&self) -> bool {
        self.background_visible
    }

    pub fn set_background_visible(&mut self, visible: bool) {
        self.background_visible = visible;
    }
}

impl<'ctx> DoActionContext<'ctx> {
    pub fn unwrap_active_canvas(&mut self) -> &mut Canvas {
        self.situation.active_canvas().unwrap()
    }
}

action! {
    AddNode[F: FnOnce(u32) -> NodeVariant] { constructor: F }
        does(ctx) {
            ctx.unwrap_active_canvas().add_node_seeded(constructor);
        }
}

action! {
    AddConnectedNode[F: FnOnce(u32) -> NodeVariant] { origin: NodeIdx, constructor: F }
        does(ctx) {
            let canvas = ctx.unwrap_active_canvas();
            let idx = canvas.add_node_seeded(constructor);
            canvas.add_edge(origin, idx);
        }
}

action! {
    AddDataframe { df: RecordBatch, path: PathBuf }
        does(ctx) {
            let name = path.file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or("Unnamed Dataframe".to_owned());
            let variant = Canvas::add_dataframe(df, ctx.registry, name, Some(path));
            ctx.unwrap_active_canvas().add_node(variant);
        }
}
action! {
    AddConnectedDataframe { origin: NodeIdx, df: RecordBatch, name: String }
        does(ctx) {
            let canvas = ctx.situation.active_canvas().unwrap();

            let variant = Canvas::add_dataframe(df, ctx.registry, name, None);
            let idx = canvas.add_node(variant);
            canvas.add_edge(origin, idx);
        }
}

action! {
    SetInteracted { idx: NodeIdx }
        does(ctx) {
            ctx.unwrap_active_canvas().set_interacted(idx);
        }
}

action! {
    AddEdge { start: NodeIdx, end: NodeIdx }
        does(ctx) {
            let canvas = ctx.unwrap_active_canvas();

            match canvas.get_node(end).variant {
                NodeVariant::TransformArg(_) => canvas.add_edge(end, start),
                _ => canvas.add_edge(start, end),
            };
        }
}

impl Canvas {
    pub fn sync_placing_node(&mut self, ui: &mut Ui) {
        if let Some(placing_idx) = self.placing_node.val()
            && let Some(cursor_screen_pos) = ui.input(|i| i.pointer.latest_pos())
        {
            // Sync its location with the pointer
            let cursor_world_pos = self.view_state.val().screen_to_world(cursor_screen_pos);
            self.get_node_mut(placing_idx).location = cursor_world_pos;
        }
    }

    pub fn draw_fluent(&self, ui: &mut Ui, actions: &mut Actions, registry: &mut Registry) {
        let (response, painter) =
            ui.allocate_painter(ui.available_size_before_wrap(), Sense::DRAG | Sense::CLICK);
        let canvas_rect = response.rect;
        let theme = LIGHT_THEME;

        if canvas_rect != *self.cached_draw_surface.val() {
            self.view_state
                .modify(|state| state.update_surface(canvas_rect));
        }

        if self.background_visible {
            self.draw_background(canvas_rect, &painter, &theme);
        }

        // Draw all connections between nodes
        let layout_context = LayoutContext {
            scale: self.view_state.val().scale(),
        };
        for &cur_idx in self.indices_by_depth.val().iter() {
            let source_node = self.get_node(cur_idx);
            let source_location_screen =
                self.view_state.val().world_to_screen(source_node.location);
            let source_rect = source_node
                .variant
                .rect(layout_context, source_location_screen);

            // For each origin, draw connections to all targets
            for outgoing_edge in self.node_edges(cur_idx) {
                let target_idx = outgoing_edge.target();
                let target_node = self.get_node(target_idx);
                let target_location_screen =
                    self.view_state.val().world_to_screen(target_node.location);

                if matches!(target_node.variant, NodeVariant::TransformArg(_)) {
                    unreachable!(
                        "Connections to `TransformArg` should always be from the arg to the target"
                    );
                }

                let target_rect = target_node
                    .variant
                    .rect(layout_context, target_location_screen);

                let (source_pos, target_pos) =
                    Node::nearest_boundary_point(source_rect, target_rect);
                painter.line_segment(
                    [source_pos, target_pos],
                    Stroke {
                        color: target_node
                            .variant
                            .override_edge_color()
                            .or(source_node.variant.override_edge_color())
                            .unwrap_or(theme.border.color),
                        ..theme.border
                    },
                );
            }
        }

        // Draw current edge
        match self.connecting_nodes.val() {
            NodeConnectionState::None => {}
            NodeConnectionState::Searching => {}
            NodeConnectionState::One(origin, ref mut current_target) => {
                let origin_node = self.get_node(origin);
                let origin_location_screen =
                    self.view_state.val().world_to_screen(origin_node.location);

                let origin_pos = origin_node
                    .variant
                    .rect(layout_context, origin_location_screen)
                    .center();

                let stroke = Stroke {
                    color: origin_node
                        .variant
                        .override_edge_color()
                        .unwrap_or(theme.border.color),
                    ..theme.border
                };

                match *current_target {
                    None => {
                        if let Some(last_cursor_pos) = ui.input(|input| input.pointer.latest_pos())
                        {
                            painter.line_segment([origin_pos, last_cursor_pos], stroke);
                        }
                    }
                    Some(target) => {
                        let target_node = self.get_node(target);
                        let target_location_screen =
                            self.view_state.val().world_to_screen(target_node.location);

                        let target_pos = target_node
                            .variant
                            .rect(layout_context, target_location_screen)
                            .center();

                        painter.line_segment(
                            [origin_pos, target_pos],
                            Stroke {
                                color: target_node
                                    .variant
                                    .override_edge_color()
                                    .unwrap_or(stroke.color),
                                ..stroke
                            },
                        );
                    }
                }

                // Set to None to find new target
                *current_target = None;
            }
        }

        // Draw all nodes
        'node_loop: for cur_idx in self.indices_by_depth.val().iter().copied() {
            let node = self.get_node(cur_idx);

            if matches!(node.variant, NodeVariant::TransformArg(_)) {
                continue 'node_loop;
            }

            let node_screen_location = self.view_state.val().world_to_screen(node.location);
            let mut draw_context = DrawContext {
                index: cur_idx,
                id: node.id,
                screen_location: node_screen_location,
                action_queue: actions,
                layout: layout_context,
                registry,
                graph: &self.graph,
                ui,
                theme: &theme,
            };

            let node_rect = node.variant.rect(layout_context, node_screen_location);
            node.variant.step(&mut draw_context);

            // At least part of the node is visible
            if canvas_rect.contains(node_screen_location) || canvas_rect.intersects(node_rect) {
                // Draw yourself, I COMMAND you
                node.variant.draw(&mut draw_context);

                // Handle creating node connections
                match self.connecting_nodes.val() {
                    NodeConnectionState::None => {}
                    NodeConnectionState::Searching => {
                        if let Some((idx, true)) = node.variant.edge_target(&mut draw_context) {
                            self.connecting_nodes
                                .set(NodeConnectionState::One(idx, None));
                        }
                    }
                    NodeConnectionState::One(origin_idx, _) => {
                        match node.variant.edge_target(&mut draw_context) {
                            None => {}
                            Some((idx, false)) => {
                                self.connecting_nodes
                                    .set(NodeConnectionState::One(origin_idx, Some(idx)));
                            }
                            Some((idx, true)) => {
                                draw_context.action_queue.push(AddEdge {
                                    start: origin_idx,
                                    end: idx,
                                });
                                self.connecting_nodes.set(NodeConnectionState::None);
                            }
                        }
                    }
                }
            }
        }

        if self.placing_node.val().is_some() {
            cursor_icon!(ui, Copy);
            if ui
                .interact(canvas_rect, Id::new("canvas_placement"), Sense::CLICK)
                .clicked()
            {
                self.placing_node.set(None);
            }
        }

        // If Canvas background was interacted
        if response.clicked() {
            response.request_focus();
        }
        match DrawInteraction::from(response) {
            DrawInteraction::Dragged(drag_delta) => {
                cursor_icon!(ui, Grabbing);
                self.view_state
                    .modify(|state| state.update_offset(-drag_delta / state.scale()));
            }
            DrawInteraction::Clicked => {
                self.connecting_nodes.set(NodeConnectionState::None);
            }
            _ => {}
        }
    }

    fn draw_background(&self, canvas_rect: Rect, painter: &Painter, theme: &Theme) {
        // If we are at an offset of 13, the first point should be 7 to the right.
        let offset = self.view_state.val().offset;
        let point_spacing = 30.0 * self.view_state.val().scale;

        let shift_x = offset.x % point_spacing;
        let shift_y = offset.y % point_spacing;
        let start_x = canvas_rect.left() - shift_x;
        let start_y = canvas_rect.top() - shift_y;

        let mut point_pos = egui::pos2(start_x, start_y);

        while point_pos.x <= canvas_rect.right() + point_spacing {
            while point_pos.y <= canvas_rect.bottom() + point_spacing {
                painter.add(Shape::Circle(CircleShape::filled(
                    point_pos,
                    1.5 * self.view_state.val().scale,
                    theme.faint_background,
                )));
                point_pos.y += point_spacing;
            }
            point_pos.y = start_y;
            point_pos.x += point_spacing;
        }
    }

    pub fn add_dataframe(
        df: RecordBatch,
        registry: &mut Registry,
        name: String,
        path: Option<PathBuf>,
    ) -> NodeVariant {
        let data_ref = registry.insert(RegistryItem {
            backing_file: path,
            inner: RegistryItemInner::Dataframe {
                table_name: name,
                data: df,
            },
        });

        NodeVariant::Dataframe(DataframePayload::new(data_ref))
    }

    pub fn set_interacted(&mut self, interacted_idx: NodeIdx) {
        self.indices_by_depth.modify(|indices| {
            let pos = indices
                .iter()
                .position(|&idx| idx == interacted_idx)
                .unwrap();
            indices.remove(pos);
            indices.push(interacted_idx);
        });
    }
}

#[derive(Clone, Default)]
pub struct CanvasGraph {
    inner: StableGraph<Node, (), Directed, u32>,
    cur_count: u32,
}

impl CanvasGraph {
    pub fn get_node(&self, idx: NodeIdx) -> &Node {
        self.inner.node_weight(idx).unwrap()
    }

    pub fn node_edges(&self, idx: NodeIdx) -> Edges<'_, (), Directed> {
        self.inner.edges(idx)
    }

    pub fn get_first_edge(&self, origin: NodeIdx) -> EdgeReference<'_, ()> {
        self.inner.edges(origin).next().unwrap()
    }

    pub fn node_edge_count(&self, idx: NodeIdx) -> usize {
        self.node_edges(idx).count()
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct ViewState {
    draw_surface: Rect,
    offset: Vec2,
    scale: f32,
    transform: RectTransform,
}

impl Default for ViewState {
    fn default() -> Self {
        let draw_surface = Rect::ZERO;
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
}

impl ViewState {
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

    #[inline(always)]
    pub fn reset_offset(&mut self) {
        self.offset = Vec2::ZERO;
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

#[derive(Copy, Clone, Default, PartialEq, Debug)]
pub enum NodeConnectionState {
    #[default]
    None,
    Searching,
    One(NodeIdx, Option<NodeIdx>),
}

#[must_use]
pub enum DexStateUpdate {
    None,
    CenterDesktop,
    ToggleBackgroundVisibility,
    ToggleDrawerVisibility,
    ToggleTabBarVisibility,
    TabForward,
    TabBackward,
}
