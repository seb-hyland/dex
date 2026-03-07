use arrow::array::RecordBatch;
use eframe::{
    egui::{
        self, FontId, Id, Painter, PointerButton, Pos2, Rect, Sense, Shadow, Shape, StrokeKind, Ui,
        Vec2,
        containers::{self, menu::MenuConfig},
        menu::context_menu,
    },
    emath::RectTransform,
    epaint::CircleShape,
};
use petgraph::{Directed, stable_graph::StableGraph};
use std::{path::PathBuf, sync::Arc};

use crate::theme::Theme;

pub type NodeIdx = petgraph::graph::NodeIndex<u32>;

pub struct Canvas {
    pub graph: StableGraph<Node, (), Directed, u32>,
    pub indices: Vec<NodeIdx>,
    pub opened_nodes: Vec<NodeIdx>,
    pub view_state: ViewState,
    pub newly_added_node: Option<NodeIdx>,
}

pub struct Node {
    pub location: Pos2,
    pub payload: NodePayload,
}

pub enum NodePayload {
    Dataframe {
        name: String,
        df: RecordBatch,
        path: Option<PathBuf>,
    },
    Transform {
        name: String,
        code: String,
    },
}

pub struct ViewState {
    pub offset: Vec2,
    pub scale: f32,
}

impl NodePayload {
    const PADDING: Vec2 = Vec2::splat(20.0);

    pub fn name(&'_ self) -> &'_ str {
        match self {
            NodePayload::Dataframe { name, .. } => name,
            NodePayload::Transform { name, .. } => name,
        }
    }

    fn get_galley(&self, ui: &Ui, scale: f32) -> Arc<egui::Galley> {
        let name = self.name();
        let theme: Theme = ui.ctx().into();
        ui.fonts_mut(|f| {
            f.layout_no_wrap(
                name.to_owned(),
                FontId::proportional(28.0 * scale),
                theme.text,
            )
        })
    }

    fn draw_node(&self, ui: &mut Ui, painter: &Painter, rect: Rect, scale: f32) {
        let theme: Theme = ui.ctx().into();
        let galley = self.get_galley(ui, scale);

        painter.rect(
            rect,
            2.0,
            theme.faint_background,
            theme.border,
            StrokeKind::Middle,
        );
        let text_pos = rect.center() - galley.size() / 2.0;
        painter.galley(text_pos, galley, theme.text);
    }

    fn draw_node_dragged(&self, ui: &mut Ui, painter: &Painter, rect: Rect, scale: f32) {
        let theme: Theme = ui.ctx().into();
        let galley = self.get_galley(ui, scale);

        painter.add(
            Shadow {
                offset: [5; 2],
                blur: 10,
                spread: 5,
                color: theme.faint_background.gamma_multiply(0.85),
            }
            .as_shape(rect, 2.0),
        );
        painter.rect(
            rect,
            2.0,
            theme.faint_background,
            theme.border,
            StrokeKind::Middle,
        );

        let text_pos = rect.center() - galley.size() / 2.0;
        painter.galley(text_pos, galley, theme.text);
    }

    fn nearest_boundary_point(&self, dir: Vec2, ui: &Ui, global_scale: f32) -> Pos2 {
        let size = self.get_galley(ui, global_scale).size() + Self::PADDING;
        let half_size = size / 2.0;

        let x_ratio = half_size.x / dir.x.abs();
        let y_ratio = half_size.y / dir.y.abs();
        let scale = x_ratio.min(y_ratio);

        (dir * scale).to_pos2()
    }
}

impl Canvas {
    pub fn draw(&mut self, ui: &mut Ui) {
        let (response, painter) =
            ui.allocate_painter(ui.available_size_before_wrap(), Sense::drag());
        let canvas_rect = response.rect;

        // If we are at an offset of 13, the first point should be 7 to the right.
        let offset = self.view_state.offset;
        let point_spacing = 30.0 * self.view_state.scale;
        let theme: Theme = ui.ctx().into();

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

        let world_rect = Rect::from_center_size(
            self.view_state.offset.to_pos2(),
            canvas_rect.size() / self.view_state.scale,
        );
        let to_screen = RectTransform::from_to(world_rect, canvas_rect);

        let mut node_dragged = self.newly_added_node.is_some();

        let mut dragged_node = None;
        for cur_idx in self.indices.iter().copied() {
            let node_id = Id::new("node").with(cur_idx);
            let node = self.graph.node_weight_mut(cur_idx).unwrap();

            let galley = node.payload.get_galley(ui, self.view_state.scale);
            let size = (galley.size() + NodePayload::PADDING) * self.view_state.scale;
            let screen_pos = to_screen.transform_pos(node.location);
            let node_rect = Rect::from_center_size(screen_pos, size);

            let node_resp = ui.interact(node_rect, node_id, Sense::all());
            node_resp.context_menu(|ui| {
                ui.set_min_width(120.0);
                ui.button("Transform node");
            });

            // If we are currently adding a new node and it is this one, sync location with the pointer
            let is_new_node = self
                .newly_added_node
                .map(|new_node_idx| new_node_idx == cur_idx)
                .unwrap_or(false);

            if is_new_node {
                if let Some(pos) = ui.input(|state| state.pointer.latest_pos()) {
                    // Transform from screen to world state
                    node.location = to_screen.inverse().transform_pos(pos);
                }
                if node_resp.clicked() {
                    self.newly_added_node = None;
                }
            }

            if self.newly_added_node.is_none() && node_resp.dragged() {
                node.location += node_resp.drag_delta() / self.view_state.scale;

                node_dragged = true;
            }
            if node_resp.clicked() && !is_new_node {
                self.opened_nodes.push(cur_idx);
            }

            if node_resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                if self.newly_added_node.is_none()
                    && ui.input(|input| input.pointer.primary_pressed())
                {
                    dragged_node = Some(cur_idx);
                }
            }

            if node_resp.hovered() && ui.input(|input| input.pointer.primary_down()) || is_new_node
            {
                node.payload
                    .draw_node_dragged(ui, &painter, node_rect, self.view_state.scale);
            } else {
                node.payload
                    .draw_node(ui, &painter, node_rect, self.view_state.scale);
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

        if response.dragged_by(PointerButton::Primary) && !node_dragged {
            self.view_state.offset -= response.drag_delta() / self.view_state.scale;
        }

        if response.hovered() {
            let zoom_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if zoom_delta != 0.0 {
                let zoom_factor = (zoom_delta / 200.0).exp();
                self.view_state.scale *= zoom_factor;
            }
        }
    }

    pub fn add_dataframe(&mut self, path: PathBuf, df: RecordBatch) {
        let node = Node {
            location: Pos2::ZERO,
            payload: NodePayload::Dataframe {
                name: path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default(),
                df,
                path: Some(path),
            },
        };
        let node_idx = self.graph.add_node(node);
        self.indices.push(node_idx);
        self.newly_added_node = Some(node_idx);
    }
}
