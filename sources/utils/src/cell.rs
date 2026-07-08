use std::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
    rc::Rc,
};

use crate::rc::Shared;

#[derive(Default, Clone)]
pub struct Rigid<T: Copy>(Shared<Cell<T>>);

impl<T: Copy> Rigid<T> {
    pub fn new(value: T) -> Self {
        Self(Shared::new(Cell::new(value)))
    }

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
pub struct Transient<T>(Cell<Shared<T>>);

impl<T> Clone for Transient<T> {
    fn clone(&self) -> Self {
        Self(Cell::new(self.0.get_cloned()))
    }
}

impl<T> Transient<T> {
    pub fn new(value: T) -> Self {
        Self(Cell::new(Shared::new(value)))
    }

    pub fn set(&self, val: T) {
        self.0.set(Shared::new(val));
    }

    pub fn val(&self) -> Shared<T> {
        self.0.get_cloned()
    }
}

impl<T: Clone> Transient<T> {
    pub fn modify<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        self.0.update(|current_val| {
            let rc_mut = Rc::make_mut(&mut current_val.0);
            f(rc_mut)
        })
    }
}

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
        // SAFETY: the slot is always initialized outside of `update`'s transient window

        let old = mem::replace(unsafe { &mut *self.value.get() }, MaybeUninit::new(val));
        drop(unsafe { old.assume_init() });
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
