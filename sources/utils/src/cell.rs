use std::{
    cell::{Ref, RefCell, RefMut},
    hash::Hash,
};

use petgraph::{EdgeType, csr::IndexType, prelude::StableGraph};
use rpds::HashTrieMap;
use serde::{Deserialize, Serialize};
use slotmap::SlotMap;

#[derive(Clone, Serialize, Deserialize)]
#[serde(bound = "")]
/// A cached, short-lived, reconstructable value.
pub struct Transient<T> {
    #[serde(skip)]
    inner: RefCell<Option<T>>,
}

impl<T> Transient<T> {
    #[track_caller]
    pub fn val(&self) -> Ref<'_, Option<T>> {
        self.inner.borrow()
    }

    pub fn val_or_else(&self, f: impl FnOnce() -> T) -> Ref<'_, T> {
        self.set_if_none(f);
        Ref::map(self.val(), |r| r.as_ref().unwrap())
    }

    #[track_caller]
    pub fn val_mut(&self) -> RefMut<'_, Option<T>> {
        self.inner.borrow_mut()
    }

    pub fn val_mut_or_else(&self, f: impl FnOnce() -> T) -> RefMut<'_, T> {
        self.set_if_none(f);
        RefMut::map(self.val_mut(), |r| r.as_mut().unwrap())
    }

    pub fn set(&self, val: T) {
        *self.inner.borrow_mut() = Some(val);
    }

    fn set_if_none(&self, f: impl FnOnce() -> T) {
        let is_none = self.inner.borrow().is_none();
        if is_none {
            self.set(f());
        }
    }
}

pub trait Reset {
    /// Reset any [`Transient`] values
    /// This is called sparingly; only on history 'resets'. It is NOT called between frames.
    fn reset(&self);
}

impl<T> Reset for Transient<T> {
    fn reset(&self) {
        *self.inner.borrow_mut() = None;
    }
}

#[macro_export]
macro_rules! impl_Reset_noop {
    ($($impl_type:ty),* $(,)?) => {
        $(
            impl $crate::Reset for $impl_type {
                #[inline(always)]
                fn reset(&self) {}
            }
        )*
    };
}

impl_Reset_noop!(
    (),
    bool,
    char,
    String,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    f32,
    f64,
    petgraph::stable_graph::NodeIndex<u32>,
    egui::Color32,
    egui::Pos2,
    egui::Vec2,
    egui::Rect,
    egui::Stroke,
    egui::FontId,
);

impl<T: Reset + ?Sized> Reset for Box<T> {
    fn reset(&self) {
        (**self).reset();
    }
}

impl<T: Reset> Reset for RefCell<T> {
    fn reset(&self) {
        self.borrow().reset();
    }
}

impl<T: Reset> Reset for Option<T> {
    fn reset(&self) {
        match self {
            Self::None => {}
            Self::Some(v) => v.reset(),
        }
    }
}

impl<T: Reset, const N: usize> Reset for [T; N] {
    fn reset(&self) {
        for item in self.iter() {
            item.reset();
        }
    }
}

impl<T: Reset> Reset for &[T] {
    fn reset(&self) {
        for item in self.iter() {
            item.reset();
        }
    }
}

impl<T: Reset> Reset for Vec<T> {
    fn reset(&self) {
        for item in self.iter() {
            item.reset();
        }
    }
}

impl<K: slotmap::Key, V: Reset> Reset for SlotMap<K, V> {
    fn reset(&self) {
        for value in self.values() {
            value.reset();
        }
    }
}

impl<K: Eq + Hash, V: Reset> Reset for HashTrieMap<K, V> {
    fn reset(&self) {
        for value in self.values() {
            value.reset();
        }
    }
}

impl<T: Reset, E, D: EdgeType, I: IndexType> Reset for StableGraph<T, E, D, I> {
    fn reset(&self) {
        for node in self.node_weights() {
            node.reset();
        }
    }
}

impl<T> Default for Transient<T> {
    fn default() -> Self {
        Self {
            inner: RefCell::new(None),
        }
    }
}
