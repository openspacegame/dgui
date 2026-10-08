use ahash::AHashMap as HashMap;
use generational_box::{AnyStorage, GenerationalBox, Owner, SyncStorage};
use std::{any::Any, cell::RefCell, marker::PhantomData, ops::Deref, rc::Rc};

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
/// Access after unmount, or conflicting access while borrowed, panics.
/// Handles and values can cross threads; accesses use synchronized storage.
pub struct State<T: 'static>(pub(crate) GenerationalBox<T, SyncStorage>);
impl<T> Copy for State<T> {}
impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: Send + Sync> State<T> {
    /// Borrow the value until the returned guard is dropped.
    ///
    /// Multiple reads are allowed. Drop all read guards before updating this
    /// state or unmounting its scope. Never hold a guard across `.await`.
    /// Panics on access after unmount or conflicting access on this thread.
    pub fn read(&self) -> StateRead<'_, T> {
        let borrow = BorrowGuard::enter(self.0.raw_ptr(), false);
        StateRead {
            value: self.0.read(),
            _borrow: borrow,
            _thread: PhantomData,
        }
    }

    pub fn get(self) -> T
    where
        T: Clone,
    {
        self.with(Clone::clone)
    }
    pub fn with<R>(self, read: impl FnOnce(&T) -> R) -> R {
        read(&self.read())
    }
    pub fn set(self, value: T) {
        self.update(|stored| *stored = value);
    }
    pub fn update<R>(self, update: impl FnOnce(&mut T) -> R) -> R {
        let _guard = BorrowGuard::enter(self.0.raw_ptr(), true);
        update(&mut self.0.write())
    }
}

/// A read borrow of a State, released on drop.
///
/// This guard stays on its borrowing thread and must not cross an await or
/// outlive its mounted scope. Clone only needed data to move it into async work.
///
/// ```compile_fail
/// fn requires_send<T: Send>() {}
/// requires_send::<dgui::StateRead<'static, usize>>();
/// ```
pub struct StateRead<'a, T: Send + Sync + 'static> {
    // Release the storage lock before the thread's borrow bookkeeping.
    value: <SyncStorage as AnyStorage>::Ref<'a, T>,
    _borrow: BorrowGuard,
    _thread: PhantomData<Rc<()>>,
}

impl<T: Send + Sync> Deref for StateRead<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

pub(crate) struct Scope {
    pub mount: u64,
    // Tasks must drop before the states they can access during destruction.
    pub tasks: Option<crate::tasks::TaskSet>,
    owner: Owner<SyncStorage>,
    values: HashMap<u64, Box<dyn Any + Send + Sync>>,
}
impl Scope {
    pub fn new(mount: u64) -> Self {
        Self {
            mount,
            tasks: None,
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
