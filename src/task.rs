use core::sync::atomic::{AtomicPtr, Ordering};

use crate::Stack;


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Exited,
}

pub struct Task {
    pub id: usize,
    pub entry: usize,
    pub user_sp: usize,
    pub state: TaskState,
}

const TASK_COUNT: usize = 2;

static mut TASKS: [Task; TASK_COUNT] = [const { Task::empty() }; TASK_COUNT];

static mut USER_STACKS: [Stack; TASK_COUNT] = [const { Stack::new() }; TASK_COUNT];

pub fn user_stack_top(task_id: usize) -> usize {
    unsafe {
        (core::ptr::addr_of_mut!(USER_STACKS[task_id].0) as *mut u8)
            .add(crate::STACK_SIZE) as usize
    }
}

static CURRENT_TASK: AtomicPtr<Task> = AtomicPtr::new(core::ptr::null_mut());

pub fn init_task(id: usize, entry: usize, user_sp: usize) {
    unsafe {
        TASKS[id] = Task::new(id, entry, user_sp);
    }
}

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


pub fn schedule() -> ! {
    let task = CURRENT_TASK.load(Ordering::SeqCst);
    if task.is_null() {
        for i in 0..TASK_COUNT {
            let task = task_mut(i);
            if task.state == TaskState::Ready {
                run_task(task);
            }
        }
    } else {
        let this = unsafe { (*task).id };
        for i in (this + 1)..TASK_COUNT {
            let task = task_mut(i);
            if task.state == TaskState::Ready {
                run_task(task);
            }
        }
    }
    // No runnable tasks, kernel halted
    crate::krnl_println!("  No runnable tasks, kernel halted");
    loop {}
}

fn task_mut(id: usize) -> &'static mut Task {
    unsafe {
        let tasks = core::ptr::addr_of_mut!(TASKS) as *mut Task;
        &mut *tasks.add(id)
    }
}

fn run_task(task: &mut Task) -> ! {
    set_current_task(task);
    mark_current_task_state(TaskState::Running);
    crate::krnl_println!("Entering user mode...");
    crate::enter_user_mode(task.entry, task.user_sp);
}

pub fn exit_current_task(exit_code: usize) -> ! {
    mark_current_task_state(TaskState::Exited);

    crate::krnl_println!("  User task exited with code {}", exit_code);

    schedule();
}

impl Task {
    pub const fn empty() -> Self {
        Self {
            id: 0,
            entry: 0,
            user_sp: 0,
            state: TaskState::Exited,
        }
    }

    pub const fn new(id: usize, entry: usize, user_sp: usize) -> Self {
        Self {
            id,
            entry,
            user_sp,
            state: TaskState::Ready,
        }
    }
}
