use crate::{actions::IntoBoxedAction, prelude::*};

use egui::Id;

#[derive(Clone)]
pub struct Buffer<T: BackingValue> {
    buf: Transient<String>,
    backing: T,
    focused: Transient<bool>,
    pub id: Id,
}

pub trait BackingValue: 'static + Sized + Clone {
    fn as_string(&self) -> String;
    fn from_str(s: &str) -> Option<Self>;
}

impl<T: BackingValue> Buffer<T> {
    pub fn new(val: T, id: Id) -> Self {
        let buf = val.as_string();
        Self {
            backing: val,
            buf: Transient::from(buf),
            focused: Transient::from(false),
            id,
        }
    }

    pub fn len_chars(&self) -> usize {
        self.buf.val().chars().count()
    }

    pub fn temp_str(&self) -> Rc<String> {
        self.buf.val()
    }

    pub fn backing_value(&self) -> &T {
        &self.backing
    }

    pub fn show<R>(&self, show_fn: impl FnOnce(&mut String, Id) -> R) -> R {
        self.buf.modify(|s| show_fn(s, self.id))
    }

    pub fn resolve_pending_actions<A: IntoBoxedAction>(
        &self,
        ui: &mut Ui,
        actions: &mut Actions,
        action_creator: impl Fn(T) -> A,
    ) {
        let currently_focused = ui.memory(|mem| mem.has_focus(self.id));
        let previously_focused = self.focused.val();

        // Just lost focus
        let new_action = if !currently_focused && *previously_focused {
            match T::from_str(&self.buf.val()) {
                Some(val) => {
                    // We can update `self.backing`
                    let set_action = action_creator(val);
                    Some(set_action)
                }
                None => {
                    // Invalid input! Try again
                    self.reset();
                    None
                }
            }
        } else {
            // No action to perform; widget did not just lose focus
            None
        };
        if let Some(action) = new_action {
            actions.push(action);
        }

        // Update focus state
        self.focused.set(currently_focused);
    }

    fn reset(&self) {
        self.buf.modify(|s| *s = self.backing.as_string());
    }

    pub fn set(&mut self, new_value: T) {
        println!("Set to {}", new_value.as_string());
        self.backing = new_value;
    }
}

impl BackingValue for String {
    fn as_string(&self) -> String {
        self.clone()
    }

    fn from_str(s: &str) -> Option<Self> {
        Some(String::from(s))
    }
}

impl BackingValue for i32 {
    fn as_string(&self) -> String {
        self.to_string()
    }

    fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

impl BackingValue for f64 {
    fn as_string(&self) -> String {
        self.to_string()
    }

    fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}
