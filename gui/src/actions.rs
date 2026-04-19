use crate::{Situation, registry::Registry};

#[derive(Default)]
pub struct Actions {
    queue: Vec<Box<dyn Action>>,
}

pub trait Action {
    fn do_(self: Box<Self>, ctx: &mut DoActionContext);
}

pub struct DoActionContext<'ctx> {
    pub situation: &'ctx mut Situation,
    pub registry: &'ctx mut Registry,
    pub frame_time: f64,
}

#[macro_export]
macro_rules! action {
    (
        $action_name:ident $([ $generic:ident : $($spec:tt)* ])? {
            $($field_name:ident : $field_type: ty),* $(,)?
        } does($ctx:ident) $body:block
    ) => {
        pub struct $action_name $(< $generic : $($spec)* >)? {
            $(pub $field_name: $field_type),*
        }

        impl $(< $generic : $($spec)* >)? Action for $action_name $(< $generic >)? {
            #[allow(unused_mut)]
            fn do_(self: Box<Self>, $ctx: &mut $crate::actions::DoActionContext) {
                $(let $field_name = self.$field_name;)*
                $body
            }
        }
    };
}

pub trait IntoBoxedAction {
    fn into_boxed(self) -> Box<dyn Action>;
}

impl IntoBoxedAction for Box<dyn Action> {
    fn into_boxed(self) -> Box<dyn Action> {
        self
    }
}

impl<A: Action + 'static> IntoBoxedAction for A {
    fn into_boxed(self) -> Box<dyn Action> {
        Box::new(self)
    }
}

impl Actions {
    pub fn push<I: IntoBoxedAction>(&mut self, action: I) {
        self.queue.push(action.into_boxed());
    }

    pub fn is_dirty(&self) -> bool {
        !self.queue.is_empty()
    }

    pub fn do_all(&mut self, ctx: &mut DoActionContext) {
        for action in self.queue.drain(..) {
            action.do_(ctx);
        }
    }
}
