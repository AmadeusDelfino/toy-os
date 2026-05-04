use alloc::boxed::Box;
use core::{
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
    task::{Context, Poll},
};

pub struct Task {
    future: Pin<Box<dyn Future<Output = ()>>>,
    id: TaskId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskId(u64);

impl Task {
    pub fn new(future: impl Future<Output = ()> + 'static) -> Task {
        Task { id: TaskId::new(), future: Box::pin(future) }
    }

    pub fn id(&self) -> TaskId {
        self.id
    }

    pub fn poll(&mut self, context: &mut Context) -> Poll<()> {
        self.future.as_mut().poll(context)
    }
}

impl TaskId {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        TaskId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}
