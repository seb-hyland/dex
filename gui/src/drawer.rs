use crate::node::NodeVariant;

#[derive(Default)]
pub struct Drawer {
    pub visible: bool,
    pub items: Vec<NodeVariant>,
}

impl Drawer {
    pub const SIZE: f32 = 150.0;
}
