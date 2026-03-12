use crate::{
    modal::{CanvasModal, RenameModal},
    node::{Node, NodePayload},
    theme::Theme,
};

use std::path::PathBuf;

use arrow::array::RecordBatch;
use eframe::{
    egui::{self, CursorIcon, Id, PointerButton, Pos2, Rect, Sense, Shape, Ui, Vec2},
    emath::RectTransform,
    epaint::CircleShape,
};
use petgraph::{Directed, stable_graph::StableGraph};

pub type NodeIdx = petgraph::graph::NodeIndex<u32>;
pub type CanvasGraph = StableGraph<Node, (), Directed, u32>;
pub struct Canvas {
    pub graph: CanvasGraph,
    pub indices: Vec<NodeIdx>,
    pub opened_nodes: Vec<(NodeIdx, Option<Pos2>)>,
    pub view_state: ViewState,
    pub newly_added_node: Option<NodeIdx>,
    pub modals: Vec<CanvasModal>,
}

pub struct ViewState {
    pub offset: Vec2,
    pub scale: f32,
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
                if ui.button("Rename").clicked() {
                    let modal = CanvasModal::Rename(RenameModal {
                        index: cur_idx,
                        new_name: node.payload.name().to_string(),
                    });
                    self.modals.push(modal);
                };
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
                self.opened_nodes.push((cur_idx, None));
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
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
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
            ui.ctx().set_cursor_icon(CursorIcon::AllScroll);
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
