use ahash::AHashMap as HashMap;
use generational_box::{GenerationalBox, Owner, SyncStorage};
use std::{any::Any, cell::RefCell};

thread_local! {
    // SyncStorage waits for cross-thread access. Reject reentrant conflicting
    // access on this thread before taking that lock, avoiding self-deadlock.
    static BORROWS: RefCell<HashMap<usize, (usize, bool)>> = RefCell::default();
}

struct BorrowGuard(usize);
impl BorrowGuard {
    fn enter(pointer: *const (), write: bool) -> Self {
        let key = pointer as usize;
        BORROWS.with_borrow_mut(|borrows| {
            let entry = borrows.entry(key).or_default();
            assert!(
                !entry.1 && (!write || entry.0 == 0),
                "dgui: conflicting state access"
            );
            entry.0 += 1;
            entry.1 = write;
        });
        Self(key)
    }
}
impl Drop for BorrowGuard {
    fn drop(&mut self) {
        BORROWS.with_borrow_mut(|borrows| {
            let entry = borrows.get_mut(&self.0).unwrap();
            entry.0 -= 1;
            if entry.0 == 0 {
                borrows.remove(&self.0);
            }
        });
    }
}

/// A Copy handle to state owned by a mounted component scope.
/// Access after unmount, or conflicting access inside `with`/`update`, panics.
/// Handles and values can cross threads; accesses use synchronized storage.
pub struct State<T: 'static>(pub(crate) GenerationalBox<T, SyncStorage>);
impl<T> Copy for State<T> {}
impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: Send + Sync> State<T> {
    pub fn get(self) -> T
    where
        T: Clone,
    {
        self.with(Clone::clone)
    }
    pub fn with<R>(self, read: impl FnOnce(&T) -> R) -> R {
        let _guard = BorrowGuard::enter(self.0.raw_ptr(), false);
        read(&self.0.read())
    }
    pub fn set(self, value: T) {
        self.update(|stored| *stored = value);
    }
    pub fn update<R>(self, update: impl FnOnce(&mut T) -> R) -> R {
        let _guard = BorrowGuard::enter(self.0.raw_ptr(), true);
        update(&mut self.0.write())
    }
}

pub(crate) struct Scope {
    pub mount: u64,
    owner: Owner<SyncStorage>,
    values: HashMap<u64, Box<dyn Any + Send + Sync>>,
}
impl Scope {
    pub fn new(mount: u64) -> Self {
        Self {
            mount,
            owner: Owner::default(),
            values: HashMap::new(),
        }
    }
    pub fn state<T: Send + Sync + 'static>(
        &mut self,
        key: u64,
        init: impl FnOnce() -> T,
    ) -> State<T> {
        let value = self
            .values
            .entry(key)
            .or_insert_with(|| Box::new(State(self.owner.insert(init()))));
        *value
            .downcast_ref::<State<T>>()
            .expect("dgui: a state key was reused with a different type")
    }
}
