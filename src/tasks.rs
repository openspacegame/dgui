use ahash::AHashSet;
use futures_util::{Stream, stream::FuturesUnordered};
use generational_box::{GenerationalBox, Owner, SyncStorage};
use std::{
    future::Future,
    hash::Hash,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

type Task = Pin<Box<dyn Future<Output = ()> + Send>>;

/// A `Copy` handle for running async work owned by a component scope,
/// returned by [`Ui::tasks`](crate::Ui::tasks).
///
/// Tasks run on the UI thread, polled by [`Dgui::show`](crate::Dgui::show)
/// after callbacks run, and a task waking up requests an egui repaint. Tasks
/// only make progress while `show` is being called: hiding the runtime pauses
/// them, and unmounting the scope drops them. Spawning through a handle whose
/// scope has unmounted does nothing.
///
/// Futures may capture owned data and [`State`](crate::State)s from this scope
/// or its ancestors, which outlive the task. Don't capture states from
/// descendant scopes, which may unmount first. Since tasks are polled on the
/// UI thread, they should await I/O rather than block.
///
/// ```
/// # fn build(ui: &mut dgui::Ui<'_, '_>) {
/// use dgui::{Frame, button};
///
/// let status = ui.state("status", || String::from("idle"));
/// let tasks = ui.tasks();
/// ui.add(button("Load").on_click(move || {
///     tasks.spawn(async move {
///         status.set("loading".into());
///         // ... await some I/O ...
///         status.set("done".into());
///     });
/// }));
/// ui.add(Frame::text(status.get()));
/// # }
/// ```
#[derive(Clone, Copy)]
pub struct Tasks(GenerationalBox<Mutex<Inbox>, SyncStorage>);

impl Tasks {
    /// Starts running `future`. It is first polled at the end of the current
    /// (or, from outside `show`, the next) call to `show`.
    pub fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        if let Ok(inbox) = self.0.try_read() {
            let mut inbox = inbox.lock().unwrap();
            inbox.pending.push(Box::pin(future));
            inbox.context.request_repaint();
        }
    }

    /// Spawns the future made by `factory` the first time `key` is used in
    /// this mounted scope, and does nothing on later calls, even after that
    /// future has finished.
    ///
    /// Use this to start a load when a component first appears. To redo the
    /// work when its inputs change, put the inputs in the enclosing
    /// [`scope`](crate::Ui::scope) key: the old scope unmounts, dropping its
    /// state and tasks, and the new one starts fresh.
    pub fn spawn_once<F>(&self, key: impl Hash, factory: impl FnOnce() -> F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let started = {
            let Ok(inbox) = self.0.try_read() else {
                return;
            };
            let started = inbox.lock().unwrap().started.insert(crate::hash(key));
            started
        };
        if started {
            // A factory may itself use a Tasks handle; never call it under a lock.
            self.spawn(factory());
        }
    }
}

struct Inbox {
    pending: Vec<Task>,
    started: AHashSet<u64>,
    context: egui::Context,
}

pub(crate) struct TaskSet {
    inbox: GenerationalBox<Mutex<Inbox>, SyncStorage>,
    // Futures need Send, not Sync. The mutex preserves Dgui's Send + Sync API.
    running: Mutex<FuturesUnordered<Task>>,
    wake: Arc<Repaint>,
    // Invalidate Copy handles when the owning task set is dropped.
    _owner: Owner<SyncStorage>,
}

impl TaskSet {
    pub fn new(context: egui::Context) -> Self {
        let owner = Owner::default();
        let inbox = owner.insert(Mutex::new(Inbox {
            pending: Vec::new(),
            started: AHashSet::default(),
            context: context.clone(),
        }));
        Self {
            inbox,
            running: Mutex::new(FuturesUnordered::new()),
            wake: Arc::new(Repaint {
                context,
                ready: AtomicBool::new(true),
            }),
            _owner: owner,
        }
    }

    pub fn handle(&self) -> Tasks {
        Tasks(self.inbox)
    }

    pub fn poll(&mut self) {
        let running = self.running.get_mut().unwrap();
        let added = {
            let inbox = self.inbox.read();
            let mut inbox = inbox.lock().unwrap();
            let added = !inbox.pending.is_empty();
            for task in inbox.pending.drain(..) {
                running.push(task);
            }
            added
        };
        let ready = self.wake.ready.swap(false, Ordering::Relaxed);
        if !added && !ready {
            return;
        }
        let waker = Waker::from(self.wake.clone());
        let mut cx = Context::from_waker(&waker);
        while let Poll::Ready(Some(())) = Pin::new(&mut *running).poll_next(&mut cx) {}
        // Pending tasks can also change State. Draw those changes next frame,
        // without requesting continuous frames while all tasks are asleep.
        self.wake.context.request_repaint();
    }
}

struct Repaint {
    context: egui::Context,
    ready: AtomicBool,
}

impl Wake for Repaint {
    fn wake(self: Arc<Self>) {
        self.ready.store(true, Ordering::Relaxed);
        self.context.request_repaint();
    }
}
