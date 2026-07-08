use dyn_clone::DynClone;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::HashMap,
    ops::Deref,
    rc::Rc,
};

/**
   A wrapper type for shared, reference-counted values.
   (De)serializes cleanly with no duplication, unlike standard serde
*/
#[derive(Default)]
pub struct Shared<T: ?Sized>(pub(crate) Rc<T>);

impl<T> Shared<T> {
    pub fn new(value: T) -> Self {
        Self(Rc::new(value))
    }
}

impl<T: ?Sized> Shared<T> {
    pub fn from_rc(rc: Rc<T>) -> Self {
        Self(rc)
    }
}

impl<T: ?Sized + DynClone> Shared<T> {
    /**
       Returns a mutable reference into this [`Shared`] object if no other references exist.
       Otherwise, clones the inner data and creates a new [`Shared`] object.
    */
    pub fn make_mut(&mut self) -> &mut T {
        let Self(inner) = self;

        if Rc::get_mut(inner).is_none() {
            let this_ref: &T = inner;
            let this_cloned = Rc::from(dyn_clone::clone_box(this_ref));
            *inner = this_cloned;
        }

        Rc::get_mut(inner).expect("No additional references to this shared object should exist")
    }
}

impl<T: ?Sized> Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: ?Sized> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Shared(Rc::clone(&self.0))
    }
}

#[derive(Serialize, Deserialize)]
enum SerializedSlot<T> {
    /// The first occurrence of a shared value
    Definition(u64, T),
    /// A reference to a definition
    Reference(u64),
}

#[derive(Default)]
struct SerializationState {
    memo: HashMap<usize, u64>,
    next_idx: u64,
}
thread_local!(static SER: RefCell<SerializationState> = RefCell::new(SerializationState::default()));

impl<T: Serialize + 'static> Serialize for Shared<T> {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let key = Rc::as_ptr(&self.0) as usize;
        let existing = SER.with(|c| c.borrow().memo.get(&key).copied());
        match existing {
            Some(id) => SerializedSlot::<&T>::Reference(id).serialize(ser),
            None => {
                let id = SER.with(|c| {
                    let mut s = c.borrow_mut();
                    let id = s.next_idx;
                    s.next_idx += 1;
                    s.memo.insert(key, id);
                    id
                });
                SerializedSlot::<&T>::Definition(id, &*self.0).serialize(ser)
            }
        }
    }
}

#[derive(Default)]
struct DeserializationState {
    memo: HashMap<(TypeId, u64), Rc<dyn Any>>,
}
thread_local!(static DE: RefCell<DeserializationState> = RefCell::new(DeserializationState::default()));

impl<'de, T: Deserialize<'de> + 'static> Deserialize<'de> for Shared<T> {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        use serde::de::Error;

        match SerializedSlot::<T>::deserialize(de)? {
            SerializedSlot::Definition(id, value) => {
                let rc = Rc::new(value);
                DE.with(|c| {
                    c.borrow_mut()
                        .memo
                        .insert((TypeId::of::<T>(), id), rc.clone() as Rc<dyn Any>)
                });
                Ok(Shared(rc))
            }
            SerializedSlot::Reference(id) => {
                // We assume deserialization occurs in serialization order.
                // Not sure if this will hold up.
                let any = DE
                    .with(|c| c.borrow().memo.get(&(TypeId::of::<T>(), id)).cloned())
                    .ok_or_else(|| D::Error::custom("Dangling shared reference"))?;
                let rc = any
                    .downcast::<T>()
                    .map_err(|_| D::Error::custom("Type mismatch of shared reference"))?;
                Ok(Shared(rc))
            }
        }
    }
}

struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        SER.with(|c| *c.borrow_mut() = SerializationState::default());
        DE.with(|c| c.borrow_mut().memo.clear());
    }
}

pub fn to_json<T: Serialize>(v: &T) -> serde_json::Result<String> {
    let _g = Reset;
    SER.with(|c| *c.borrow_mut() = SerializationState::default());
    serde_json::to_string(v)
}

pub fn from_json<T: for<'de> Deserialize<'de>>(s: &str) -> serde_json::Result<T> {
    let _g = Reset;
    DE.with(|c| c.borrow_mut().memo.clear());
    serde_json::from_str(s)
}
