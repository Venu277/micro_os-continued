use super::{Task, TaskId};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use alloc::task::Wake;
use core::task::{Context, Poll, Waker};
use spin::Mutex;

struct TaskWaker {
    task_id: TaskId,
    task_queue: Arc<Mutex<VecDeque<TaskId>>>,
}

impl TaskWaker {
    fn new(task_id: TaskId, task_queue: Arc<Mutex<VecDeque<TaskId>>>) -> Waker {
        Waker::from(Arc::new(TaskWaker {
            task_id,
            task_queue,
        }))
    }
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let mut queue = self.task_queue.lock();
        if !queue.contains(&self.task_id) {
            queue.push_back(self.task_id);
        }
    }
}

pub struct Executor {
    tasks: BTreeMap<TaskId, Task>,
    task_queue: Arc<Mutex<VecDeque<TaskId>>>,
    waker_cache: BTreeMap<TaskId, Waker>,
}

impl Executor {
    pub fn new() -> Self {
        Executor {
            tasks: BTreeMap::new(),
            task_queue: Arc::new(Mutex::new(VecDeque::new())),
            waker_cache: BTreeMap::new(),
        }
    }

    pub fn spawn(&mut self, task: Task) {
        let task_id = task.id();
        self.tasks.insert(task_id, task);
        self.task_queue.lock().push_back(task_id);
    }

    fn run_ready_tasks(&mut self) {
        while let Some(task_id) = self.task_queue.lock().pop_front() {
            let task = match self.tasks.get_mut(&task_id) {
                Some(task) => task,
                None => continue, // Task already finished
            };

            let waker = self
                .waker_cache
                .entry(task_id)
                .or_insert_with(|| TaskWaker::new(task_id, self.task_queue.clone()));

            let mut context = Context::from_waker(waker);
            match task.poll(&mut context) {
                Poll::Ready(()) => {
                    // Task completed: clean up task and cached waker
                    self.tasks.remove(&task_id);
                    self.waker_cache.remove(&task_id);
                }
                Poll::Pending => {}
            }
        }
    }

    fn sleep_if_idle(&self) {
        use x86_64::instructions::interrupts::{self, enable_and_hlt};

        interrupts::without_interrupts(|| {
            if self.task_queue.lock().is_empty() {
                enable_and_hlt();
            }
        });
    }

    pub fn run(&mut self) -> ! {
        loop {
            self.run_ready_tasks();
            self.sleep_if_idle();
        }
    }
}
