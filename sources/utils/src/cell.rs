use std::{cell::UnsafeCell, rc::Rc};

use serde::{Deserialize, Serialize};

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
    fn reset(&mut self);
}

impl<T> Reset for Transient<T> {
    fn reset(&mut self) {
        *self = Self::default();
    }
}

impl<T> Default for Transient<T> {
    fn default() -> Self {
        Self {
            inner: Cell::new(None),
        }
    }
}

impl<T> Clone for Transient<T> {
    fn clone(&self) -> Self {
        // Reset the cache
        Self::default()
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
