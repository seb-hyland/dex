use crate::{
    node::{
        DrawContext, DrawInteraction, Node, NodeDynamics, NodeVariant,
        data::{DataPayload, DataframePayload},
    },
    registry::{Registry, RegistryItem, RegistryItemInner},
    theme::Theme,
};

use std::{path::PathBuf, ptr::NonNull};

use arrow::array::RecordBatch;
use eframe::{
    egui::{
        self, Context, CursorIcon, Id, Painter, PointerButton, Pos2, Rect, Sense, Shape, Ui, Vec2,
    },
    emath::RectTransform,
    epaint::CircleShape,
};
use petgraph::{Directed, stable_graph::StableGraph};

pub type NodeIdx = petgraph::graph::NodeIndex<u32>;
pub type CanvasGraph = StableGraph<Node, (), Directed, u32>;

pub struct Canvas {
    pub graph: CanvasGraph,
    pub indices: Vec<NodeIdx>,
    pub view_state: ViewState,
    pub placing_node: Option<NodeIdx>,
}

pub struct ViewState {
    draw_surface: Rect,
    offset: Vec2,
    scale: f32,
    transform: RectTransform,
}

impl Canvas {
    pub fn draw(&mut self, ui: &mut Ui, registry: &mut Registry) {
        let (response, painter) =
            ui.allocate_painter(ui.available_size_before_wrap(), Sense::drag());
        let canvas_rect = response.rect;
        let theme: Theme = ui.ctx().into();
        self.view_state.update_surface(canvas_rect);

        self.draw_background(canvas_rect, &painter, &theme);

        let mut dragged_node = None;
        let self_ptr = NonNull::from_mut(self);
        for cur_idx in self.indices.iter().copied() {
            let node = self.graph.node_weight_mut(cur_idx).unwrap();
            let node_id = Id::new("canvas_node").with(cur_idx);

            let node_location = self.view_state.world_to_screen(node.location);
            let mut draw_context = DrawContext {
                index: cur_idx,
                id: node_id,
                screen_location: node_location,
                canvas: self_ptr,
                registry,
                ui,
                painter: &painter,
                theme: &theme,
                placing: false,
            };

            // At least part of the node is visible
            let node_size = node.variant.size(&mut draw_context);
            let node_rect = Rect::from_center_size(node_location, node_size);
            if canvas_rect.contains(node_location) || canvas_rect.intersects(node_rect) {
                // If we are currently adding a new node and it is this one
                if let Some(placing_idx) = self.placing_node
                    && placing_idx == cur_idx
                {
                    draw_context.placing = true;
                };

                // Draw yourself, I COMMAND YOU
                node.variant.draw(&mut draw_context);

                if draw_context.placing {
                    // Steal interaction and sense placement
                    let resp = draw_context.ui.interact(node_rect, node_id, Sense::click());
                    if let Some(cursor_screen_pos) =
                        draw_context.ui.ctx().input(|i| i.pointer.latest_pos())
                    {
                        // Sync its location with the pointer
                        let cursor_world_pos =
                            draw_context.view_state().screen_to_world(cursor_screen_pos);
                        node.location = cursor_world_pos;
                    }
                    if resp.clicked() {
                        self.placing_node = None;
                    }
                }
            }
        }

        // Move dragged node to front
        if let Some(dragged_idx) = dragged_node {
            let pos = self
                .indices
                .iter()
                .position(|&idx| idx == dragged_idx)
                .unwrap();
            self.indices.remove(pos);
            self.indices.push(dragged_idx);
        }

        // if response.dragged_by(PointerButton::Primary) && self.placing_node.is_none() {
        if response.dragged_by(PointerButton::Primary) {
            self.view_state
                .update_offset(-response.drag_delta() / self.view_state.scale());
        }

        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::AllScroll);
            let zoom_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if zoom_delta != 0.0 {
                let zoom_factor = (zoom_delta / 200.0).exp();
                self.view_state.update_scale(zoom_factor);
                self.view_state.scale *= zoom_factor;
            }
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
        let data_idx = registry.insert(RegistryItem {
            backing_file: Some(path.clone()),
            inner: RegistryItemInner::Dataframe(df),
        });

        let node = Node {
            location: Pos2::ZERO,
            variant: NodeVariant::Data(DataPayload::Dataframe(DataframePayload {
                name: path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default(),
                data_idx,
            })),
        };
        self.add_node(node);
    }

    pub fn add_node(&mut self, node: Node) {
        let idx = self.graph.add_node(node);
        self.indices.push(idx);
        self.placing_node = Some(idx);
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

    pub fn update_surface(&mut self, draw_surface: Rect) {
        self.draw_surface = draw_surface;
        self.update_transform();
    }

    pub fn offset(&self) -> Vec2 {
        self.offset
    }

    pub fn update_offset(&mut self, delta: Vec2) {
        self.offset += delta;
        self.update_transform();
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn update_scale(&mut self, zoom_factor: f32) {
        self.scale *= zoom_factor;
        self.update_transform();
    }

    pub fn world_to_screen(&self, pos: Pos2) -> Pos2 {
        self.transform.transform_pos(pos)
    }

    pub fn screen_to_world(&self, pos: Pos2) -> Pos2 {
        self.transform.inverse().transform_pos(pos)
    }
}
