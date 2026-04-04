//! Don't end up on the list!
use crate::prelude::*;

use std::{cell::RefCell, path::PathBuf, rc::Rc};

#[derive(Default)]
pub struct Registry {
    items: Vec<Rc<RefCell<RegistryItem>>>,
}

pub struct RegistryItem {
    pub backing_file: Option<PathBuf>,
    pub inner: RegistryItemInner,
}

pub enum RegistryItemInner {
    Dataframe {
        table_name: String,
        data: RecordBatch,
    },
}

pub type RegistryHandle = usize;

impl Registry {
    pub fn insert(&mut self, item: RegistryItem) -> RegistryHandle {
        let index = self.items.len();
        self.items.push(Rc::new(RefCell::new(item)));
        index
    }

    pub fn get(&self, index: RegistryHandle) -> Option<Rc<RefCell<RegistryItem>>> {
        self.items.get(index).cloned()
    }
}
