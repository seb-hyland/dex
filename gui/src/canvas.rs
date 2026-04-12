use crate::prelude::*;
use crate::theme::Theme;
use crate::{
    node::{
        DrawContext, Node, NodeDynamics, NodeVariant, dataframe::DataframePayload,
        transform::TransformArgPayload, view::Window,
    },
    registry::{Registry, RegistryItem, RegistryItemInner},
};

use std::io::{Read, Write};
use std::path::PathBuf;
use std::ptr::NonNull;

use eframe::Frame;
use eframe::egui::Stroke;
use eframe::{
    egui::{Id, Sense, Shape},
    emath::RectTransform,
    epaint::CircleShape,
};
use petgraph::stable_graph::EdgeReference;
use petgraph::{Directed, stable_graph::StableGraph, visit::EdgeRef};
use serde::{Deserialize, Serialize};

pub type NodeIdx = petgraph::graph::NodeIndex<u32>;
pub type CanvasGraph = StableGraph<Node, (), Directed, u32>;

#[derive(Default, Serialize, Deserialize)]
pub struct Canvas {
    pub graph: CanvasGraph,
    pub indices_by_depth: Vec<NodeIdx>,
    pub view_state: ViewState,
    pub placing_node: Option<NodeIdx>,
    #[serde(skip)]
    pub connecting_nodes: NodeConnectionState,
}

#[derive(Serialize, Deserialize)]
pub struct ViewState {
    draw_surface: Rect,
    offset: Vec2,
    scale: f32,
    transform: RectTransform,
}

#[derive(Copy, Clone, Default, PartialEq, Debug)]
pub enum NodeConnectionState {
    #[default]
    None,
    Searching,
    One(NodeIdx, Option<NodeIdx>),
}

impl Canvas {
    pub fn draw(&mut self, ui: &mut Ui, registry: &mut Registry, frame: &mut Frame) {
        let (response, painter) = ui.allocate_painter(ui.available_size_before_wrap(), Sense::DRAG);
        let canvas_rect = response.rect;
        let theme = LIGHT_THEME;
        self.view_state.update_surface(canvas_rect);

        self.draw_background(canvas_rect, &painter, &theme);

        let mut command_queue = Vec::new();
        let graph_ref = DisjointGraphRef::new(&self.graph);

        // Draw all connections between nodes
        for cur_idx in self.graph.node_indices() {
            let source_node = self.graph.node_weight(cur_idx).unwrap();

            let source_location = source_node.location;
            let source_location_screen = self.view_state.world_to_screen(source_location);

            let id = Id::new("canvas_edge").with(cur_idx);
            let mut draw_context = DrawContext {
                index: cur_idx,
                screen_location: source_location_screen,
                id,
                command_queue: &mut command_queue,
                view_state: &self.view_state,
                registry,
                graph_ref,
                ui,
                theme: &theme,
            };
            let source_rect = source_node.variant.rect(&mut draw_context);

            // For each origin, draw connections to all targets
            for outgoing_edge in self.graph.edges(cur_idx) {
                let target_idx = outgoing_edge.target();
                let target_node = self.graph.node_weight(target_idx).unwrap();

                if matches!(target_node.variant, NodeVariant::TransformArg(_)) {
                    unreachable!(
                        "Connections to `TransformArg` should always be from the arg to the target"
                    );
                }

                let target_location = target_node.location;
                let target_location_screen = self.view_state.world_to_screen(target_location);

                draw_context.index = target_idx;
                draw_context.screen_location = target_location_screen;
                let target_rect = target_node.variant.rect(&mut draw_context);

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
        match self.connecting_nodes {
            NodeConnectionState::None => {}
            NodeConnectionState::Searching => {}
            NodeConnectionState::One(origin, ref mut current_target) => {
                let origin_node = self.graph.node_weight(origin).unwrap();
                let origin_location = origin_node.location;
                let origin_location_screen = self.view_state.world_to_screen(origin_location);

                let id = Id::new("canvas_connecting_edge");
                let mut draw_context = DrawContext {
                    index: origin,
                    screen_location: origin_location_screen,
                    id,
                    command_queue: &mut command_queue,
                    view_state: &self.view_state,
                    registry,
                    graph_ref,
                    ui,
                    theme: &theme,
                };
                let origin_pos = origin_node.variant.rect(&mut draw_context).center();

                let stroke = Stroke {
                    color: origin_node
                        .variant
                        .override_edge_color()
                        .unwrap_or(theme.border.color),
                    ..theme.border
                };

                match *current_target {
                    None => {
                        if let Some(last_cursor_pos) = draw_context
                            .ui
                            .ctx()
                            .input(|input| input.pointer.latest_pos())
                        {
                            painter.line_segment([origin_pos, last_cursor_pos], stroke);
                        }
                    }
                    Some(target) => {
                        let target_node = self.graph.node_weight(target).unwrap();
                        let target_location = target_node.location;
                        let target_location_screen =
                            self.view_state.world_to_screen(target_location);

                        draw_context.index = target;
                        draw_context.screen_location = target_location_screen;
                        let target_pos = target_node.variant.rect(&mut draw_context).center();
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
        'node_loop: for cur_idx in self.indices_by_depth.iter().copied() {
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
                graph_ref,
                ui,
                theme: &theme,
            };

            let node_rect = node.variant.rect(&mut draw_context);
            // At least part of the node is visible
            if canvas_rect.contains(node_location) || canvas_rect.intersects(node_rect) {
                // Draw yourself, I COMMAND you
                node.variant.draw(&mut draw_context);

                // Handle creating node connections
                match self.connecting_nodes {
                    NodeConnectionState::None => {}
                    NodeConnectionState::Searching => {
                        if let Some((idx, true)) = node.variant.edge_target(&mut draw_context) {
                            self.connecting_nodes = NodeConnectionState::One(idx, None)
                        }
                    }
                    NodeConnectionState::One(origin_idx, _) => {
                        match node.variant.edge_target(&mut draw_context) {
                            None => {}
                            Some((idx, false)) => {
                                self.connecting_nodes =
                                    NodeConnectionState::One(origin_idx, Some(idx))
                            }
                            Some((idx, true)) => {
                                draw_context.command_queue.push(CanvasCommand::AddEdge {
                                    start: origin_idx,
                                    end: idx,
                                });
                                self.connecting_nodes = NodeConnectionState::None;
                            }
                        }
                    }
                }
            }
        }

        if let Some(placing_idx) = self.placing_node {
            if let Some(cursor_screen_pos) = ui.input(|i| i.pointer.latest_pos()) {
                // Sync its location with the pointer
                let cursor_world_pos = self.view_state.screen_to_world(cursor_screen_pos);
                self.graph.node_weight_mut(placing_idx).unwrap().location = cursor_world_pos;
            }
            cursor_icon!(ui, Copy);
            if ui
                .interact(canvas_rect, Id::new("canvas_placement"), Sense::CLICK)
                .clicked()
            {
                self.placing_node = None;
            }
        }

        let mut interacted_node = None;
        let frame_time = ui.input(|i| i.time);

        for command in command_queue {
            command.exe(self, registry, &mut interacted_node, frame_time);
        }

        // Move dragged node to front
        if let Some(interacted_idx) = interacted_node {
            let pos = self
                .indices_by_depth
                .iter()
                .position(|&idx| idx == interacted_idx)
                .unwrap();
            self.indices_by_depth.remove(pos);
            self.indices_by_depth.push(interacted_idx);
        }

        // If Canvas background was dragged
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

    pub fn add_dataframe(
        &mut self,
        df: RecordBatch,
        registry: &mut Registry,
        name: String,
        path: Option<PathBuf>,
    ) -> NodeIdx {
        let data_ref = registry.insert(RegistryItem {
            backing_file: path,
            inner: RegistryItemInner::Dataframe {
                table_name: name,
                data: df,
            },
        });

        let node = NodeVariant::Dataframe(DataframePayload {
            data_ref,
            scroll_to: None,
            highlighted_row: None,
            view: Window::default(),
        });
        self.add_node(node)
    }

    pub fn add_node(&mut self, node: NodeVariant) -> NodeIdx {
        let idx = self.add_node_noplacing(node);
        self.placing_node = Some(idx);
        idx
    }

    pub fn add_node_noplacing(&mut self, node: NodeVariant) -> NodeIdx {
        let idx = self.graph.add_node(Node {
            location: Pos2::ZERO,
            variant: node,
        });
        self.indices_by_depth.push(idx);
        idx
    }

    pub fn serialize_to_paths(&self, prefix: &std::path::Path) {
        let file_txt = std::fs::File::create(prefix.with_added_extension("dext")).unwrap();
        serde_json::to_writer(file_txt, self).unwrap();

        let mut file_bin = std::fs::File::create(prefix.with_added_extension("dex")).unwrap();
        let bytes = postcard::to_stdvec(self).unwrap();
        file_bin.write_all(&bytes).unwrap();
    }

    pub fn load_from_path(&mut self, path: PathBuf) {
        let mut file = std::fs::File::open(&path).unwrap();
        let mut bytes = Vec::new();
        let data = match path.extension().and_then(|ext| ext.to_str()) {
            Some("dext") => serde_json::from_reader(file).unwrap(),
            Some("dex") => {
                file.read_to_end(&mut bytes);
                postcard::from_bytes(&bytes).unwrap()
            }
            _ => unimplemented!(),
        };
        *self = data
    }
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

#[derive(Clone, Copy)]
pub struct DisjointGraphRef {
    canvas_ref: NonNull<CanvasGraph>,
}

impl DisjointGraphRef {
    pub fn new(graph: &CanvasGraph) -> Self {
        Self {
            canvas_ref: NonNull::from_ref(graph),
        }
    }

    pub fn get(&self, idx: NodeIdx) -> Option<&Node> {
        unsafe { self.canvas_ref.as_ref().node_weight(idx) }
    }

    pub fn edge_count(&self, idx: NodeIdx) -> usize {
        unsafe { self.canvas_ref.as_ref().edges(idx).count() }
    }

    pub fn get_edge(&self, idx: NodeIdx) -> Option<EdgeReference<'_, ()>> {
        unsafe { self.canvas_ref.as_ref().edges(idx).next() }
    }
}

pub enum CanvasCommand {
    MoveNode {
        idx: NodeIdx,
        delta: Vec2,
    },
    AddNode {
        origin: NodeIdx,
        node: NodeVariant,
    },
    AddDataframe {
        origin: NodeIdx,
        df: RecordBatch,
        name: String,
    },
    SetInteracted {
        idx: NodeIdx,
    },
    AddEdge {
        start: NodeIdx,
        end: NodeIdx,
    },
    ScrollTable {
        table_node: NodeIdx,
        row: usize,
    },
    AddTransformArg {
        origin: NodeIdx,
    },
    UpdateTransformArgLocation {
        idx: NodeIdx,
        new_rect: Rect,
    },
}

impl CanvasCommand {
    fn exe(
        self,
        canvas: &mut Canvas,
        registry: &mut Registry,
        interacted_node: &mut Option<NodeIdx>,
        frame_time: f64,
    ) {
        match self {
            Self::MoveNode { idx, delta } => {
                *interacted_node = Some(idx);
                canvas.graph.node_weight_mut(idx).unwrap().location += delta;
            }
            Self::AddNode { origin, node } => {
                let idx = canvas.add_node(node);
                canvas.graph.add_edge(origin, idx, ());
            }
            Self::AddDataframe { origin, df, name } => {
                let idx = canvas.add_dataframe(df, registry, name, None);
                canvas.graph.add_edge(origin, idx, ());
            }
            Self::SetInteracted { idx } => {
                *interacted_node = Some(idx);
            }
            Self::AddEdge { start, end } => {
                match canvas.graph.node_weight(end).unwrap().variant {
                    NodeVariant::TransformArg(_) => canvas.graph.add_edge(end, start, ()),
                    _ => canvas.graph.add_edge(start, end, ()),
                };
            }
            Self::ScrollTable { table_node, row } => {
                let df_node = canvas
                    .graph
                    .node_weight_mut(table_node)
                    .unwrap()
                    .variant
                    .unwrap_dataframe_mut();
                df_node.scroll_to = Some(row);
                df_node.highlighted_row = Some((row, frame_time));
            }
            Self::AddTransformArg { origin } => {
                let next_color = if let NodeVariant::Transform(ref mut t) =
                    canvas.graph.node_weight_mut(origin).unwrap().variant
                {
                    let next_color = t
                        .last_color
                        .map(Theme::palette_next)
                        .unwrap_or(Theme::COLOR_PALETTE[0]);
                    t.last_color = Some(next_color);
                    next_color
                } else {
                    unreachable!("AddTransformArg called by non transform node");
                };

                let new_idx =
                    canvas.add_node_noplacing(NodeVariant::TransformArg(TransformArgPayload {
                        cached_rect: Rect::ZERO,
                        color: next_color,
                    }));

                if let NodeVariant::Transform(ref mut t) =
                    canvas.graph.node_weight_mut(origin).unwrap().variant
                {
                    t.args.last_mut().unwrap().node = Some(new_idx);
                } else {
                    unreachable!("AddTransformArg called by non transform node");
                }
            }
            Self::UpdateTransformArgLocation { idx, new_rect } => {
                let node = canvas.graph.node_weight_mut(idx).unwrap();
                if let NodeVariant::TransformArg(t) = &mut node.variant {
                    t.cached_rect = new_rect;
                } else {
                    unreachable!("UpdateTransformArgLocation called on non transform-arg node")
                }
            }
        }
    }
}
