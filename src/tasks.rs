use ahash::AHashSet;
use futures_util::{Stream, stream::FuturesUnordered};
use std::{
    future::Future,
    hash::Hash,
    pin::Pin,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

type Task = Pin<Box<dyn Future<Output = ()> + Send>>;

/// An owned handle for starting work in a component's mounted scope.
///
/// Capture this handle in ordinary widget callbacks. Futures may capture owned
/// data and states from this scope or its ancestors, but not shorter-lived states.
/// The scope polls tasks during `show`; hiding the runtime pauses them, and
/// unmounting drops them. Spawning through an unmounted handle does nothing.
#[derive(Clone)]
pub struct Tasks(Weak<Mutex<Inbox>>);

impl Tasks {
    /// Start an operation. The future is first polled after this draw's callbacks.
    pub fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        if let Some(inbox) = self.0.upgrade() {
            let mut inbox = inbox.lock().unwrap();
            inbox.pending.push(Box::pin(future));
            inbox.context.request_repaint();
        }
    }

    /// Start once per key for this mount, even if the operation completes.
    /// The factory runs only on the first call. Changing inputs should change
    /// the enclosing scope key, dropping its old state and tasks together.
    pub fn spawn_once<F>(&self, key: impl Hash, factory: impl FnOnce() -> F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let Some(inbox) = self.0.upgrade() else {
            return;
        };
        if inbox.lock().unwrap().started.insert(crate::hash(key)) {
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
    inbox: Arc<Mutex<Inbox>>,
    // Futures need Send, not Sync. The mutex preserves Dgui's Send + Sync API.
    running: Mutex<FuturesUnordered<Task>>,
    wake: Arc<Repaint>,
}

impl TaskSet {
    pub fn new(context: egui::Context) -> Self {
        Self {
            inbox: Arc::new(Mutex::new(Inbox {
                pending: Vec::new(),
                started: AHashSet::default(),
                context: context.clone(),
            })),
            running: Mutex::new(FuturesUnordered::new()),
            wake: Arc::new(Repaint {
                context,
                ready: AtomicBool::new(true),
            }),
        }
    }

    pub fn handle(&self) -> Tasks {
        Tasks(Arc::downgrade(&self.inbox))
    }

    pub fn poll(&mut self) {
        let running = self.running.get_mut().unwrap();
        let added = {
            let mut inbox = self.inbox.lock().unwrap();
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
