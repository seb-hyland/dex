use crate::prelude::*;
use crate::{
    node::{DrawContext, NodeDynamics, view::ViewNode},
    registry::{RegistryHandle, RegistryItemInner},
    table::draw_record_batch,
};

pub struct DataframeView {
    pub data_ref: RegistryHandle,
    pub view: ViewNode,
}

impl NodeDynamics for DataframeView {
    fn draw(&mut self, ctx: &mut DrawContext<'_>) {
        let item = ctx.registry.get(self.data_ref).unwrap();

        if let RegistryItemInner::Dataframe {
            ref mut table_name,
            ref data,
        } = item.borrow_mut().inner
        {
            self.view.show(ctx, table_name, |ui| {
                draw_record_batch(ui, data);
            });
        } else {
            unreachable!("Data table view for non-df registry item")
        }
    }

    #[inline(always)]
    fn rect(&self, ctx: &mut DrawContext<'_>) -> Rect {
        self.view.rects(ctx.ui, ctx.screen_location).1
    }
}
