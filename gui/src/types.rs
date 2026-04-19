use crate::prelude::*;

use std::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
};

/**
    A re-implementation of [`std::cell::Cell`] with support for:
    - [`Clone`]-based get operations for arbitrary `T` (see [`Self::get_cloned`])
    - Transactional updates for arbitrary `T` (see [`Self::update`])
*/
struct Cell<T> {
    value: UnsafeCell<MaybeUninit<T>>,
}

impl<T: Default> Default for Cell<T> {
    fn default() -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::new(T::default())),
        }
    }
}

impl<T> Cell<T> {
    fn new(val: T) -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::new(val)),
        }
    }

    fn set(&self, val: T) {
        mem::replace(unsafe { &mut *self.value.get() }, MaybeUninit::new(val));
    }
}

struct PanicGuard<'slot, T> {
    val: MaybeUninit<T>,
    container: &'slot mut MaybeUninit<T>,
}

impl<'slot, T> Drop for PanicGuard<'slot, T> {
    fn drop(&mut self) {
        // Take the value
        let val = mem::replace(&mut self.val, MaybeUninit::uninit());
        // Re-initialize the container
        *self.container = val;
    }
}

struct AbortGuard;

impl Drop for AbortGuard {
    fn drop(&mut self) {
        std::process::abort()
    }
}

impl<T> Cell<T> {
    fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        unsafe {
            let container = &mut *self.value.get();
            // If f panics, there is no recovering
            let guard = AbortGuard;

            let mut val = mem::replace(container, MaybeUninit::uninit()).assume_init();
            let res = f(&mut val);

            container.write(val);
            // We made it 😌
            mem::forget(guard);
            res
        }
    }
}

impl<T: Clone> Cell<T> {
    fn get_cloned(&self) -> T {
        unsafe { (*self.value.get()).assume_init_ref() }.clone()
    }
}

#[derive(Default, Clone)]
pub struct Rigid<T: Copy>(Rc<Cell<T>>);

impl<T: Copy> From<T> for Rigid<T> {
    fn from(value: T) -> Self {
        Self(Rc::new(Cell::new(value)))
    }
}

impl<T: Copy> Rigid<T> {
    pub fn val(&self) -> T {
        self.0.get_cloned()
    }

    pub fn set(&self, val: T) {
        self.0.set(val);
    }

    pub fn modify<U>(&self, mut f: impl FnMut(&mut T) -> U) -> U {
        let mut new_val: T = self.val();
        let out = f(&mut new_val);
        self.set(new_val);
        out
    }
}

#[derive(Default)]
pub struct Transient<T>(Cell<Rc<T>>);

impl<T> Clone for Transient<T> {
    fn clone(&self) -> Self {
        Self(Cell::new(self.0.get_cloned()))
    }
}

impl<T> From<T> for Transient<T> {
    fn from(value: T) -> Self {
        Self(Cell::new(Rc::from(value)))
    }
}

impl<T> Transient<T> {
    pub fn val(&self) -> Rc<T> {
        self.0.get_cloned()
    }

    pub fn set(&self, val: T) {
        self.0.set(Rc::new(val));
    }
}

impl<T: Clone> Transient<T> {
    pub fn modify<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        self.0.update(|current_val| {
            let rc_mut = Rc::make_mut(current_val);
            f(rc_mut)
        })
    }
}
