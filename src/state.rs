use generational_box::{GenerationalBox, Owner};
use std::{any::Any, collections::HashMap};

/// A Copy handle to state owned by a mounted component scope.
/// Access after unmount, or conflicting access inside `with`/`update`, panics.
/// Handles and their values stay on the UI thread.
pub struct State<T: 'static>(pub(crate) GenerationalBox<T>);
impl<T> Copy for State<T> {}
impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> State<T> {
    pub fn get(self) -> T
    where
        T: Clone,
    {
        self.0.read().clone()
    }
    pub fn with<R>(self, read: impl FnOnce(&T) -> R) -> R {
        read(&self.0.read())
    }
    pub fn set(self, value: T) {
        self.0.set(value);
    }
    pub fn update<R>(self, update: impl FnOnce(&mut T) -> R) -> R {
        update(&mut self.0.write())
    }
}

pub(crate) struct Scope {
    pub mount: u64,
    owner: Owner,
    values: HashMap<u64, Box<dyn Any>>,
}
impl Scope {
    pub fn new(mount: u64) -> Self {
        Self {
            mount,
            owner: Owner::default(),
            values: HashMap::new(),
        }
    }
    pub fn state<T: 'static>(&mut self, key: u64, init: impl FnOnce() -> T) -> State<T> {
        let value = self
            .values
            .entry(key)
            .or_insert_with(|| Box::new(State(self.owner.insert(init()))));
        *value
            .downcast_ref::<State<T>>()
            .expect("dgui: a state key was reused with a different type")
    }
}
