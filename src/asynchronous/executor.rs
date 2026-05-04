//! Cooperative async executor used by the kernel's main loop.
//!
//! The executor owns all spawned kernel tasks and polls only tasks whose
//! `TaskId` appears in `task_queue`. Futures make progress by returning
//! `Poll::Pending` after registering the provided `Waker`; when the event they
//! depend on happens, that waker re-enqueues the task ID. This keeps idle tasks
//! out of the polling hot path and lets interrupt-driven producers, such as the
//! keyboard scancode path, wake a task that registered interest in the event.
//!
//! ## Scheduling model
//!
//! - Scheduling is cooperative. A task runs until its future returns
//!   `Poll::Pending` or `Poll::Ready(())`; there is no preemption, time slicing,
//!   priority, or per-task budget here.
//! - `spawn` inserts a task into `tasks` and immediately marks it ready by
//!   pushing its ID into `task_queue`.
//! - `run_ready_tasks` drains the ready queue. IDs for completed or removed
//!   tasks are ignored, which also makes duplicate wakeups harmless apart from
//!   the extra queue entries they consume.
//! - Completed tasks are removed together with their cached `Waker`. The current
//!   task abstraction only supports `Future<Output = ()>`, so there is no result
//!   propagation or join handle.
//!
//! ## Wake contract
//!
//! A `Waker` created by this executor does not poll directly. It only pushes the
//! corresponding `TaskId` into `task_queue`; the main executor loop performs the
//! actual poll later. This is the boundary that allows wakeups from interrupt
//! context without touching the `tasks` map or the `waker_cache`.
//!
//! Futures must follow the normal async contract: before returning
//! `Poll::Pending`, they must arrange for the supplied waker to be called once
//! progress is possible. If a future returns `Pending` without registering a
//! wake path, it can sleep forever. If it wakes repeatedly before being polled,
//! the queue can contain duplicate IDs.
//!
//! ## Idle and interrupt contract
//!
//! `sleep_if_idle` disables interrupts before checking whether the ready queue
//! is empty. This ordering prevents a lost-wakeup race where an interrupt wakes a
//! task after the empty check but before `hlt`; in that race the CPU could go to
//! sleep even though work is already queued. When the queue is empty,
//! `enable_and_hlt` atomically enables interrupts and halts until the next
//! interrupt. When work is queued, interrupts are simply re-enabled and the loop
//! polls again.
//!
//! The executor itself is single-threaded and owns `tasks`/`waker_cache`
//! exclusively. Only `task_queue` is shared with wakers, so any future change
//! that lets wakeups mutate executor-owned state must revisit the interrupt and
//! locking story deliberately.
//!
//! ## Capacity and initialization
//!
//! `task_queue` is bounded to 100 entries. A wake when the queue is full panics
//! today, so adding high-frequency wake sources, many tasks, or wake storms
//! requires revisiting the capacity/backpressure policy instead of assuming
//! enqueue always succeeds.
//!
//! Construct the executor only after heap allocation is initialized. Tasks are
//! heap-backed, wakers are stored in `Arc`s, and the queue/map structures also
//! rely on allocation.

use super::{Task, TaskId};
use alloc::{collections::BTreeMap, sync::Arc, task::Wake};
use core::task::{Context, Poll, Waker};
use crossbeam_queue::ArrayQueue;

/// Waker implementation for one task.
///
/// Clones of the resulting `Waker` may be held by futures or interrupt-facing
/// helpers. Waking is intentionally limited to enqueueing `task_id`; polling
/// remains centralized in `Executor::run_ready_tasks`.
struct TaskWaker {
    task_id: TaskId,
    task_queue: Arc<ArrayQueue<TaskId>>,
}

/// Minimal executor for heap-backed kernel async tasks.
///
/// `tasks` is the ownership map, `task_queue` is the ready set represented as a
/// FIFO queue of task IDs, and `waker_cache` keeps stable wakers so futures are
/// not forced to allocate a fresh `Arc<TaskWaker>` on every poll.
pub struct Executor {
    tasks: BTreeMap<TaskId, Task>,
    task_queue: Arc<ArrayQueue<TaskId>>,
    waker_cache: BTreeMap<TaskId, Waker>,
}

impl TaskWaker {
    fn new(task_id: TaskId, task_queue: Arc<ArrayQueue<TaskId>>) -> Waker {
        Waker::from(Arc::new(TaskWaker { task_id, task_queue }))
    }

    /// Marks the associated task ready to be polled by the main executor loop.
    fn wake_task(&self) {
        self.task_queue.push(self.task_id).expect("task_queue full");
    }
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_task();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wake_task();
    }
}

impl Executor {
    pub fn new() -> Self {
        Executor {
            tasks: BTreeMap::new(),
            task_queue: Arc::new(ArrayQueue::new(100)),
            waker_cache: BTreeMap::new(),
        }
    }

    pub fn run(&mut self) -> ! {
        loop {
            self.run_ready_tasks();
            self.sleep_if_idle();
        }
    }

    fn sleep_if_idle(&self) {
        use x86_64::instructions::interrupts::{self, enable_and_hlt};

        // Keep interrupts disabled between the empty check and `hlt` so an
        // interrupt cannot enqueue work in the gap and leave the CPU asleep
        // with a ready task already waiting.
        interrupts::disable();
        if self.task_queue.is_empty() {
            enable_and_hlt();
        } else {
            interrupts::enable();
        }
    }

    pub fn spawn(&mut self, task: Task) {
        let task_id = task.id();

        if self.tasks.get(&task_id).is_some() {
            //FIXME Maybe... spawn -> Result<T, E>? Make sense? idk
            panic!("task with same ID already in tasks ")
        }

        self.tasks.insert(task_id, task);

        self.task_queue.push(task_id).expect("queue full");
    }

    fn run_ready_tasks(&mut self) {
        // destructure `self` to avoid borrow checker errors
        let Self { tasks, task_queue, waker_cache } = self;

        while let Some(task_id) = task_queue.pop() {
            let task = match tasks.get_mut(&task_id) {
                Some(task) => task,
                None => continue, // task no longer exists
            };

            // Reuse wakers across polls. Besides avoiding repeated allocation,
            // this gives futures a stable waker identity for comparisons such
            // as `Waker::will_wake`.
            let waker = waker_cache.entry(task_id).or_insert_with(|| TaskWaker::new(task_id, task_queue.clone()));
            let mut context = Context::from_waker(waker);
            match task.poll(&mut context) {
                Poll::Ready(()) => {
                    // task done -> remove it and its cached waker
                    tasks.remove(&task_id);
                    waker_cache.remove(&task_id);
                }
                Poll::Pending => {}
            }
        }
    }
}

//FIXME: Think about how to test the loop
#[cfg(test)]
mod tests {
    use spin::Mutex;

    use super::*;

    #[test_case]
    fn insert_task_when_call_spawn() {
        let mut executor = Executor::new();
        executor.spawn(Task::new(async {}));
        assert_eq!(executor.task_queue.len(), 1);
    }

    #[test_case]
    fn check_run_ready_task_operation() {
        let mut executor = Executor::new();
        executor.spawn(Task::new(async {}));
        executor.run_ready_tasks();
        assert_eq!(executor.task_queue.len(), 0);
        assert_eq!(executor.waker_cache.len(), 0);
    }

    //FIXME: I need to think in a better name
    #[test_case]
    fn check_arc_reference_correctness() {
        struct ArcTest {
            pub number: u8,
        }
        let arc_test = Arc::new(Mutex::new(ArcTest { number: 0 }));
        let mut executor = Executor::new();
        let runner = async |larc: Arc<Mutex<ArcTest>>| {
            larc.lock().number = 10;
        };
        executor.spawn(Task::new(runner(Arc::clone(&arc_test))));
        executor.run_ready_tasks();
        assert_eq!(arc_test.lock().number, 10);
    }
}
