//! Don't end up on the list!

use std::{path::PathBuf, sync::Arc};

use arrow::array::RecordBatch;

#[derive(Default)]
pub struct Registry {
    items: Vec<Arc<RegistryItem>>,
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
        self.items.push(Arc::new(item));
        index
    }

    pub fn get(&self, index: RegistryHandle) -> Option<Arc<RegistryItem>> {
        self.items.get(index).cloned()
    }
}
