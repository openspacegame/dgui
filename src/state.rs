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

/// A `Copy` handle to a value owned by a component scope, created by
/// [`Ui::state`](crate::Ui::state).
///
/// Capture the handle in closures, event callbacks and async tasks freely; it
/// stays valid until its scope unmounts. Handles and values can cross threads,
/// since the storage is synchronized.
///
/// Accesses are checked like a `RefCell`: any number of reads at once, or a
/// single write. Writes from event callbacks are seen the next time the tree
/// is built.
///
/// # Panics
///
/// Every accessor panics if the scope has unmounted, or if the access
/// conflicts with another one in progress on the same thread, such as calling
/// [`update`](Self::update) inside [`with`](Self::with) on the same state.
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

    /// Returns a clone of the value.
    pub fn get(self) -> T
    where
        T: Clone,
    {
        self.with(Clone::clone)
    }
    /// Calls `read` with a reference to the value and returns its result.
    pub fn with<R>(self, read: impl FnOnce(&T) -> R) -> R {
        read(&self.read())
    }
    /// Replaces the value.
    pub fn set(self, value: T) {
        self.update(|stored| *stored = value);
    }
    /// Calls `update` with a mutable reference to the value and returns its
    /// result.
    pub fn update<R>(self, update: impl FnOnce(&mut T) -> R) -> R {
        let _guard = BorrowGuard::enter(self.0.raw_ptr(), true);
        update(&mut self.0.write())
    }
}

/// A read borrow of a [`State`], returned by [`State::read`] and released on
/// drop.
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
