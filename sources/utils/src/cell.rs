use std::{
    cell::{RefCell, UnsafeCell},
    hash::Hash,
    rc::Rc,
};

use petgraph::{EdgeType, csr::IndexType, prelude::StableGraph};
use rpds::HashTrieMap;
use serde::{Deserialize, Serialize};
use slotmap::SlotMap;

#[derive(Default, Clone)]
pub struct Rigid<T: Copy>(Rc<Cell<T>>);

impl<T: Copy> Rigid<T> {
    pub fn new(value: T) -> Self {
        Self(Rc::new(Cell::new(value)))
    }

    pub fn val(&self) -> T {
        self.0.get_cloned()
    }

    pub fn set(&self, val: T) {
        self.0.set(val);
    }
}

#[derive(Serialize, Deserialize)]
#[serde(bound = "")]
/// A cached, short-lived, reconstructable value.
pub struct Transient<T> {
    #[serde(skip)]
    inner: Cell<Option<Rc<T>>>,
}

pub trait Reset {
    /// Reset any [`Transient`] values
    fn reset(&self);
}

impl<T> Reset for Transient<T> {
    fn reset(&self) {
        self.inner.set(None);
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

impl<T: Clone> Clone for Transient<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Cell::new(self.inner.get_cloned()),
        }
    }
}

impl<T> Default for Transient<T> {
    fn default() -> Self {
        Self {
            inner: Cell::new(None),
        }
    }
}

impl<T> Transient<T> {
    pub fn set(&self, val: T) {
        self.inner.set(Some(Rc::new(val)));
    }

    pub fn val(&self) -> Option<Rc<T>> {
        self.inner.get_cloned()
    }
}

/**
    A re-implementation of [`std::cell::Cell`] with support for:
    - [`Clone`]-based get operations for arbitrary `T` (see [`Self::get_cloned`])
*/
struct Cell<T> {
    value: UnsafeCell<T>,
}

impl<T: Default> Default for Cell<T> {
    fn default() -> Self {
        Self {
            value: UnsafeCell::new(T::default()),
        }
    }
}

impl<T> Cell<T> {
    fn new(val: T) -> Self {
        Self {
            value: UnsafeCell::new(val),
        }
    }

    fn set(&self, val: T) {
        // SAFETY: no other references can exist
        unsafe { *self.value.get() = val };
    }
}

impl<T: Clone> Cell<T> {
    fn get_cloned(&self) -> T {
        unsafe { &*self.value.get() }.clone()
    }
}
