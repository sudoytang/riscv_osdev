use core::sync::atomic::{AtomicPtr, Ordering};

use crate::program::{ProgramEntry, USER_STACK_TOP, embedded_user_bin};

// Callee-saved registers + ra/sp. Layout must match switch_context offsets.
#[repr(C)]
pub struct TaskContext {
    ra:  usize,  // offset 0
    sp:  usize,  // offset 8
    s0:  usize,  // offset 16
    s1:  usize,  // offset 24
    s2:  usize,  // offset 32
    s3:  usize,  // offset 40
    s4:  usize,  // offset 48
    s5:  usize,  // offset 56
    s6:  usize,  // offset 64
    s7:  usize,  // offset 72
    s8:  usize,  // offset 80
    s9:  usize,  // offset 88
    s10: usize,  // offset 96
    s11: usize,  // offset 104
}

impl TaskContext {
    pub const fn zero() -> Self {
        Self {
            ra: 0, sp: 0,
            s0: 0, s1: 0, s2: 0, s3: 0, s4: 0, s5: 0,
            s6: 0, s7: 0, s8: 0, s9: 0, s10: 0, s11: 0,
        }
    }
}

// Saves callee-saved regs of `current`, restores callee-saved regs of `next`,
// then `ret` (which jumps to next.ra). Both args are passed in a0/a1 per RISC-V
// calling convention.
#[unsafe(naked)]
unsafe extern "C" fn switch_context(current: *mut TaskContext, next: *mut TaskContext) {
    core::arch::naked_asm!(
        "sd ra,    0(a0)",
        "sd sp,    8(a0)",
        "sd s0,   16(a0)",
        "sd s1,   24(a0)",
        "sd s2,   32(a0)",
        "sd s3,   40(a0)",
        "sd s4,   48(a0)",
        "sd s5,   56(a0)",
        "sd s6,   64(a0)",
        "sd s7,   72(a0)",
        "sd s8,   80(a0)",
        "sd s9,   88(a0)",
        "sd s10,  96(a0)",
        "sd s11, 104(a0)",

        "ld ra,    0(a1)",
        "ld sp,    8(a1)",
        "ld s0,   16(a1)",
        "ld s1,   24(a1)",
        "ld s2,   32(a1)",
        "ld s3,   40(a1)",
        "ld s4,   48(a1)",
        "ld s5,   56(a1)",
        "ld s6,   64(a1)",
        "ld s7,   72(a1)",
        "ld s8,   80(a1)",
        "ld s9,   88(a1)",
        "ld s10,  96(a1)",
        "ld s11, 104(a1)",
        "ret",
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Unused,
    Ready,
    Running,
    Exited,
}

pub struct Task {
    pub id: usize,
    pub entry: usize,
    pub user_sp: usize,
    pub user_page_table_pa: usize,
    pub kernel_stack_top: usize,
    pub saved_user_sp: usize,
    pub state: TaskState,
    pub context: TaskContext,
}

const MAX_TASKS: usize = 8;

static mut TASKS: [Task; MAX_TASKS] = [const { Task::empty() }; MAX_TASKS];
static mut SCHEDULER_CONTEXT: TaskContext = TaskContext::zero();
static CURRENT_TASK: AtomicPtr<Task> = AtomicPtr::new(core::ptr::null_mut());

/// Create a new runnable task by copying `entry`'s program image into private pages.
pub fn spawn(entry: &ProgramEntry) -> Option<usize> {
    let slot = find_free_slot()?;
    let program = embedded_user_bin();

    let stack_pa = crate::mm::alloc_frame();
    let user_sp_va = USER_STACK_TOP;
    let page_table = crate::mm::create_user_address_space(program, stack_pa);
    let user_page_table_pa = page_table as usize;

    let kernel_stack_pa = crate::mm::alloc_frame();
    let kernel_stack_top = kernel_stack_pa + 4096;

    let entry_va = entry.entry_va();
    let mut task = Task::new(slot, entry_va, user_sp_va, user_page_table_pa, kernel_stack_top);
    task.context.ra = task_first_run as *const () as usize;
    task.context.sp = kernel_stack_top;
    task.state = TaskState::Ready;

    unsafe {
        TASKS[slot] = task;
    }

    crate::krnl_println!(
        "[spawn]  spawn slot {}: {} @ {:#x}",
        slot,
        entry.name,
        entry_va
    );

    Some(slot)
}

fn find_free_slot() -> Option<usize> {
    for i in 0..MAX_TASKS {
        let task = task_mut(i);
        if task.state == TaskState::Unused || task.state == TaskState::Exited {
            return Some(i);
        }
    }
    None
}

// Entry point for a task's first run. Called via `ret` from switch_context.
// At this point sp is already set to the task's kernel_stack_top.
extern "C" fn task_first_run() {
    let task = CURRENT_TASK.load(Ordering::SeqCst);
    let (entry, user_sp, kernel_stack_top) = unsafe {
        ((*task).entry, (*task).user_sp, (*task).kernel_stack_top)
    };
    crate::krnl_println!("[task_first_run] Entering user mode...");
    crate::enter_user_mode(entry, user_sp, kernel_stack_top);
}

fn set_current_task(task: *mut Task) {
    CURRENT_TASK.store(task, Ordering::SeqCst);
}

fn task_mut(id: usize) -> &'static mut Task {
    unsafe {
        let tasks = core::ptr::addr_of_mut!(TASKS) as *mut Task;
        &mut *tasks.add(id)
    }
}

fn switch_satp(user_page_table_pa: usize) {
    let satp = (8usize << 60) | (user_page_table_pa >> 12);
    unsafe {
        core::arch::asm!(
            "csrw satp, {satp}",
            "sfence.vma",
            satp = in(reg) satp,
        )
    }
}

/// Run the scheduler until no Ready tasks remain.
pub fn schedule_until_idle() {
    loop {
        // After returning from a task: free the kernel stack if the task exited.
        let prev = CURRENT_TASK.load(Ordering::SeqCst);
        if !prev.is_null() {
            unsafe {
                if (*prev).state == TaskState::Exited {
                    let ks = (*prev).kernel_stack_top;
                    if ks != 0 {
                        crate::mm::free_frame(ks - 4096);
                        (*prev).kernel_stack_top = 0;
                    }
                }
            }
        }
        set_current_task(core::ptr::null_mut());

        // Round-robin: start searching from the task after the one that just ran.
        let start = if prev.is_null() {
            0
        } else {
            (unsafe { (*prev).id } + 1) % MAX_TASKS
        };

        let mut found = false;
        for i in 0..MAX_TASKS {
            let idx = (start + i) % MAX_TASKS;
            let task = task_mut(idx);
            if task.state == TaskState::Ready {
                task.state = TaskState::Running;
                set_current_task(task);
                switch_satp(task.user_page_table_pa);

                // For resumed tasks, restore sscratch to the saved user sp so
                // the trap epilogue can correctly swap back to the user stack.
                // For first-run tasks (saved_user_sp == 0) task_first_run
                // will set sscratch via enter_user_mode.
                if task.saved_user_sp != 0 {
                    unsafe {
                        core::arch::asm!(
                            "csrw sscratch, {sp}",
                            sp = in(reg) task.saved_user_sp,
                        );
                    }
                }

                unsafe {
                    switch_context(
                        core::ptr::addr_of_mut!(SCHEDULER_CONTEXT),
                        core::ptr::addr_of_mut!(task.context),
                    );
                }
                // Returned here after the task called sched().
                found = true;
                break;
            }
        }

        if !found {
            crate::krnl_println!("[schedule_until_idle]  No runnable tasks");
            return;
        }
    }
}

// Switches from the current task back to the scheduler. Called by
// yield_current_task and exit_current_task (directly).
fn sched() {
    let task = CURRENT_TASK.load(Ordering::SeqCst);
    assert!(!task.is_null());

    // sscratch currently holds the current task's user sp (set by trap entry).
    // Save it so schedule() can restore it when this task is resumed.
    let saved_user_sp: usize;
    unsafe {
        core::arch::asm!("csrr {}, sscratch", out(reg) saved_user_sp);
        (*task).saved_user_sp = saved_user_sp;

        switch_context(
            core::ptr::addr_of_mut!((*task).context),
            core::ptr::addr_of_mut!(SCHEDULER_CONTEXT),
        );
    }
    // Returns here when schedule_until_idle() picks this task again.
}

pub fn yield_current_task() {
    let task = CURRENT_TASK.load(Ordering::SeqCst);
    unsafe { (*task).state = TaskState::Ready; }
    sched();
    // Returns here when this task is resumed. The trap epilogue will sret back
    // to user mode after tf.sepc has been advanced by the outer trap_handler.
}

pub fn exit_current_task(exit_code: usize) -> ! {
    let task = CURRENT_TASK.load(Ordering::SeqCst);
    assert!(!task.is_null());

    unsafe { (*task).state = TaskState::Exited; }

    crate::krnl_println!("[exit_current_task]  User task exited with code {}", exit_code);

    // Switch back to kernel page table before freeing user mappings.
    crate::mm::switch_to_kernel_page_table();
    unsafe {
        crate::mm::free_user_page_table((*task).user_page_table_pa);
        (*task).user_page_table_pa = 0;
    }

    // Switch directly to the scheduler. schedule_until_idle() will free the
    // kernel stack on the next iteration (after we are no longer running on it).
    unsafe {
        switch_context(
            core::ptr::addr_of_mut!((*task).context),
            core::ptr::addr_of_mut!(SCHEDULER_CONTEXT),
        );
    }
    loop {}
}

impl Task {
    pub const fn empty() -> Self {
        Self {
            id: 0,
            entry: 0,
            user_sp: 0,
            user_page_table_pa: 0,
            kernel_stack_top: 0,
            saved_user_sp: 0,
            state: TaskState::Unused,
            context: TaskContext::zero(),
        }
    }

    pub const fn new(
        id: usize,
        entry: usize,
        user_sp: usize,
        user_page_table_pa: usize,
        kernel_stack_top: usize,
    ) -> Self {
        Self {
            id,
            entry,
            user_sp,
            user_page_table_pa,
            kernel_stack_top,
            saved_user_sp: 0,
            state: TaskState::Ready,
            context: TaskContext::zero(),
        }
    }
}
