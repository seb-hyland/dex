use arrow::array::RecordBatch;
use eframe::egui::{CornerRadius, FontId, Galley, Pos2, Rect, Shape, Vec2};
use egui_graphs::{DisplayEdge, DisplayNode, DrawContext, EdgeProps, Graph, NodeProps};
use petgraph::{Directed, EdgeType, csr::IndexType};
use std::{rc::Rc, sync::Arc};

use crate::theme::Theme;

#[derive(Clone)]
pub enum NodeInner {
    Dataframe { data: RecordBatch },
    Transformation {},
}
#[derive(Clone)]
pub struct Node {
    pub inner: NodeInner,
    pub name: String,
}
pub type SharedNode = Rc<Node>;

pub type DisplayGraph = Graph<SharedNode, (), Directed, u32, GraphNode, GraphEdge>;

#[derive(Clone)]
pub struct GraphNode {
    pub center: Pos2,
    pub selected: bool,
    pub value: SharedNode,
    cached_zoom: f32,
    pub galley: Option<Arc<Galley>>,
}

impl GraphNode {
    const PADDING: Vec2 = Vec2::splat(12.0);

    pub fn size(&self) -> Vec2 {
        self.galley
            .clone()
            .map(|galley| galley.size() / self.cached_zoom)
            .unwrap_or_else(|| Vec2::splat(120.0))
            + Self::PADDING
    }
}

impl From<NodeProps<SharedNode>> for GraphNode {
    fn from(props: NodeProps<SharedNode>) -> Self {
        Self {
            center: props.location(),
            selected: props.selected,
            value: props.payload.clone(),
            cached_zoom: 1.,
            galley: None, // This will be cached later in `shapes`
        }
    }
}

impl<E, Ty, Ix> DisplayNode<SharedNode, E, Ty, Ix> for GraphNode
where
    E: Clone,
    Ty: EdgeType,
    Ix: IndexType,
{
    fn update(&mut self, state: &NodeProps<SharedNode>) {
        self.center = state.location();
        self.value = state.payload.clone();
        self.selected = state.selected;
    }

    fn shapes(&mut self, ctx: &DrawContext<'_>) -> Vec<Shape> {
        let canvas_center = ctx.meta.canvas_to_screen_pos(self.center);

        let theme: Theme = ctx.ctx.into();
        let current_zoom = ctx.meta.zoom;
        let font_id = FontId::proportional(12.0 * current_zoom);

        let galley = if let Some(galley) = self.galley.clone()
            // Only if zoom has not changed
            && current_zoom == self.cached_zoom
        {
            galley
        } else {
            let galley = ctx
                .ctx
                .fonts_mut(|f| f.layout_no_wrap(self.value.name.clone(), font_id, theme.text));
            self.galley = Some(galley.clone());
            self.cached_zoom = current_zoom;
            galley
        };

        let screen_padding = Self::PADDING * current_zoom;
        let screen_size = galley.size() + screen_padding;
        let rect = Rect::from_center_size(canvas_center, screen_size);
        let node_rect = Shape::rect_filled(
            rect,
            match self.value.inner {
                NodeInner::Dataframe { .. } => CornerRadius::same(0),
                // Don't forget to scale corner radius, else it vanishes when zoomed out
                NodeInner::Transformation {} => CornerRadius::same(20 * current_zoom as u8),
            },
            theme.faint_background,
        );

        // Top-left of first char
        let text_pos = canvas_center - galley.size() / 2.0;
        let text_shape = Shape::galley(text_pos, galley, theme.text);

        vec![node_rect, text_shape]
    }

    fn closest_boundary_point(&self, dir: Vec2) -> Pos2 {
        let size = self.size();
        let half_size = size / 2.0;

        let x_ratio = half_size.x / dir.x.abs();
        let y_ratio = half_size.y / dir.y.abs();
        let scale = x_ratio.min(y_ratio);

        self.center + dir * scale
    }

    fn is_inside(&self, pos: Pos2) -> bool {
        let rect = Rect::from_center_size(self.center, self.size());
        rect.contains(pos)
    }
}

#[derive(Clone, Default)]
pub struct GraphEdge;

impl<E: Clone> From<EdgeProps<E>> for GraphEdge {
    fn from(_props: EdgeProps<E>) -> Self {
        Self
    }
}

impl<N, E, Ty, Ix, D> DisplayEdge<N, E, Ty, Ix, D> for GraphEdge
where
    N: Clone,
    E: Clone,
    Ty: EdgeType,
    Ix: IndexType,
    D: DisplayNode<N, E, Ty, Ix>,
{
    fn update(&mut self, _state: &egui_graphs::EdgeProps<E>) {}

    fn shapes(
        &mut self,
        start: &egui_graphs::Node<N, E, Ty, Ix, D>,
        end: &egui_graphs::Node<N, E, Ty, Ix, D>,
        ctx: &DrawContext<'_>,
    ) -> Vec<Shape> {
        let start_pos = start.location();
        let end_pos = end.location();

        let dir = (end_pos - start_pos).normalized();

        let start_point = start.display().closest_boundary_point(dir);
        let end_point = end.display().closest_boundary_point(-dir);

        let start_screen = ctx.meta.canvas_to_screen_pos(start_point);
        let end_screen = ctx.meta.canvas_to_screen_pos(end_point);

        let theme: Theme = ctx.ctx.into();
        vec![Shape::line_segment(
            [start_screen, end_screen],
            theme.border,
        )]
    }

    fn is_inside(
        &self,
        _start: &egui_graphs::Node<N, E, Ty, Ix, D>,
        _end: &egui_graphs::Node<N, E, Ty, Ix, D>,
        _pos: Pos2,
    ) -> bool {
        // Edges will never be selectable
        false
    }
}
