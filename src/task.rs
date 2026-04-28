use core::sync::atomic::{AtomicPtr, Ordering};

pub enum TaskState {
    Ready,
    Running,
    Exited,
}

pub struct Task {
    pub entry: usize,
    pub user_sp: usize,
    pub state: TaskState,
}

// Our kernel can only handle one task once now.
static CURRENT_TASK: AtomicPtr<Task> = AtomicPtr::new(core::ptr::null_mut());

pub fn set_current_task(task: &mut Task) {
    CURRENT_TASK.store(task, Ordering::SeqCst);
}

pub fn mark_current_task_state(task_state: TaskState) {
    let task = CURRENT_TASK.load(Ordering::SeqCst);
    if !task.is_null() {
        unsafe {
            (*task).state = task_state;
        }
    }
}

pub fn exit_current_task(exit_code: usize) -> ! {
    mark_current_task_state(TaskState::Exited);

    crate::krnl_println!("  User task exited with code {}", exit_code);

    schedule();
}

fn schedule() -> ! {
    crate::krnl_println!("  No runnable tasks, kernel halted");
    loop {}
}

impl Task {
    pub const fn new(entry: usize, user_sp: usize) -> Self {
        Self {
            entry,
            user_sp,
            state: TaskState::Ready,
        }
    }
}
