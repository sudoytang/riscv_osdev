# Step 9: Temporary Static Task Scheduler

The ninth milestone adds a fixed-size task scheduler.

This step is intentionally temporary.

The current scheduler exists to demonstrate that the kernel can keep more than one task record, run one user task, handle `SYS_EXIT`, and then run the next ready task. It is not the final task model for this kernel.

## Important Warning

This implementation is a teaching scaffold.

It will almost certainly change after virtual memory is introduced.

The current task mechanism still assumes:

```text
user entry addresses are raw kernel-visible addresses
user stacks are statically allocated kernel objects
SYS_WRITE can read user pointers directly
tasks do not own separate address spaces
tasks are never resumed after yielding
the scheduler only runs each task once in a fixed order
```

Those assumptions are acceptable before paging, but they are not acceptable for a real process model.

Once Sv39 virtual memory is enabled, the task structure will need to grow or be redesigned around:

```text
per-task page tables
user virtual stack addresses
mapped user code pages
validated user memory access
saved trap frames or contexts
clear ownership of user address spaces
```

So this step should be treated as a bridge, not a stable architecture.

## What Changed

Before this step, the kernel had one task:

```text
kernel_main
  -> create one Task
  -> run it
  -> SYS_EXIT marks it Exited
  -> scheduler halts
```

Now the kernel initializes two tasks:

```rust
task::init_task(0, user_main_0 as *const () as usize, task::user_stack_top(0));
task::init_task(1, user_main_1 as *const () as usize, task::user_stack_top(1));

task::schedule();
```

Each task has its own user stack:

```rust
static mut USER_STACKS: [Stack; TASK_COUNT] = [const { Stack::new() }; TASK_COUNT];
```

The kernel chooses a stack by task id:

```rust
pub fn user_stack_top(task_id: usize) -> usize {
    unsafe {
        (core::ptr::addr_of_mut!(USER_STACKS[task_id].0) as *mut u8)
            .add(crate::STACK_SIZE) as usize
    }
}
```

## Runtime Task Initialization

The task table is global:

```rust
static mut TASKS: [Task; TASK_COUNT] = [const { Task::empty() }; TASK_COUNT];
```

It cannot be initialized with real entry addresses and stack addresses at compile time.

This does not work:

```rust
static mut TASKS: [Task; 2] = [
    Task::new(0, user_main_0 as *const () as usize, user_stack_top(0)),
    Task::new(1, user_main_1 as *const () as usize, user_stack_top(1)),
];
```

Rust does not allow pointer-to-integer casts during const evaluation, and `user_stack_top` is not a const function.

Instead, `TASKS` starts with empty entries:

```rust
pub const fn empty() -> Self {
    Self {
        id: 0,
        entry: 0,
        user_sp: 0,
        state: TaskState::Exited,
    }
}
```

Then `kernel_main` fills the real task data at runtime:

```rust
pub fn init_task(id: usize, entry: usize, user_sp: usize) {
    unsafe {
        TASKS[id] = Task::new(id, entry, user_sp);
    }
}
```

## Scheduler Behavior

The scheduler is fixed-order and single-pass:

```text
task 0 Ready -> run task 0
task 0 exits
task 1 Ready -> run task 1
task 1 exits
no Ready tasks -> halt
```

It does not implement round-robin scheduling.

It does not resume a task after it stops running.

It does not handle a task that loops forever without calling `SYS_EXIT`.

The current `schedule` function scans the global task table:

```rust
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

    crate::krnl_println!("  No runnable tasks, kernel halted");
    loop {}
}
```

The second scan starts after the current task id. This is enough for sequential execution, but it is not a general scheduler.

## The Critical Bug: Scheduling From the Trap Stack

After implementing this step, we noticed a more serious issue: this scheduler is not just temporary, it is architecturally wrong.

`SYS_EXIT` is reached through the trap path:

```text
user task
  -> ecall
  -> __trap_entry
  -> save TrapFrame on the kernel trap stack
  -> trap_handler
  -> handle_syscall
  -> exit_current_task
  -> schedule
  -> enter_user_mode(next task)
```

The normal trap return path would eventually restore the saved registers, release the `TrapFrame` from the kernel trap stack, and execute `sret`.

But the current `SYS_EXIT` path never returns to that restore path.

Instead, it calls `schedule()` from inside the trap handler and directly enters the next user task:

```text
trap frame for task 0 is not unwound by the normal restore path
schedule enters task 1
task 1 later traps using the same kernel trap stack top
the new trap frame reuses or overwrites the same trap-frame slot
```

Tracing `tf` addresses shows this clearly. In this two-task demo, the trap frame address stays the same across task 0 and task 1 U-mode traps. So the current behavior is not simple unbounded stack growth for every `SYS_EXIT`.

The actual bug is more subtle: the task 0 exit trap frame is abandoned without a structured return, and task 1 is entered by jumping away from task 0's trap-handler call chain. Later traps then reuse the same trap-frame slot because `enter_user_mode` resets `sscratch` to the kernel trap stack top.

The demo does not fail because `SYS_EXIT` never needs to resume the old user task, and the old trap frame is no longer used. That does not make the design correct.

This is why the current scheduler must not be extended into `SYS_YIELD`, timer interrupts, or real context switching. A real design needs an explicit way to leave the trap path without abandoning the saved trap frame, or a real context-switch mechanism that treats saved trap frames and kernel stacks as first-class task state.

This bug is the main reason to stop here and move to the next foundation: virtual memory, followed by a redesigned task/context model.

## Why the Task Table Is Global

`SYS_EXIT` is handled from the trap path.

When a user task calls `SYS_EXIT`, the CPU traps into the kernel and runs on the kernel trap stack. The code is no longer executing in the normal `kernel_main` local stack frame.

That means the scheduler cannot naturally access a local variable such as:

```rust
let mut tasks = [...]
```

For this stage, the task table is global kernel state:

```rust
static mut TASKS: [Task; TASK_COUNT]
```

This is not elegant, but it matches the current early-kernel constraints.

Later, this global table may become a real process table, a scheduler queue, or a structure protected by kernel synchronization.

## Avoiding static_mut_refs

Rust 2024 compatibility denies creating a mutable reference to an entire `static mut`:

```rust
for task in unsafe { &mut TASKS } {
    ...
}
```

The current code uses a raw pointer helper instead:

```rust
fn task_mut(id: usize) -> &'static mut Task {
    unsafe {
        let tasks = core::ptr::addr_of_mut!(TASKS) as *mut Task;
        &mut *tasks.add(id)
    }
}
```

This still relies on unsafe global mutable state. It is acceptable for the current single-core, non-preemptive teaching kernel, but it is another reason this code should be revisited later.

## User Tasks

There are now two user entry points:

```rust
pub extern "C" fn user_main_0() -> ! {
    user_println!("Task 0");
    user_println!("Hello from U-mode via syscall!");
    user_println!("Let's trigger a trap of illegal instruction...");
    unsafe {
        core::arch::asm!(".word 0");
    }
    user_println!("Illegal instruction trap handled.");
    user_println!("Bye!");
    user_exit(0);
}

pub extern "C" fn user_main_1() -> ! {
    user_println!("Task 1");
    user_println!("Hello from U-mode via syscall!");
    user_println!("Bye!");
    user_exit(0);
}
```

Task 0 still exercises the illegal-instruction trap path.

Task 1 proves that the scheduler can continue to another ready task after task 0 exits.

## Expected Flow

The runtime path is now:

```text
kernel_main
  -> init task 0
  -> init task 1
  -> schedule

schedule
  -> run task 0

task 0
  -> SYS_WRITE
  -> illegal instruction trap
  -> SYS_EXIT

exit_current_task
  -> mark task 0 Exited
  -> schedule

schedule
  -> run task 1

task 1
  -> SYS_WRITE
  -> SYS_EXIT

exit_current_task
  -> mark task 1 Exited
  -> schedule

schedule
  -> no runnable tasks
  -> halt
```

## Why We Stop Here

It is tempting to add `SYS_YIELD`, round-robin scheduling, or context switching next.

That would be premature.

Without virtual memory, every task still shares the same raw address space. Without a saved task context, a task cannot really yield and later resume from where it stopped. Building those features now would create a scheduler shape that will likely be torn apart after paging.

So the project should stop deepening this scheduler here.

The next major milestone should be virtual memory.

## Next Step

The next step should be Sv39 virtual memory bootstrap:

```text
build an early kernel page table
identity-map the current kernel memory
map UART MMIO
write satp
run sfence.vma
continue executing with paging enabled
```

After that, the task design should be revisited.

The future task structure will probably need fields such as:

```text
page table root
saved trap frame
user virtual stack top
kernel stack
task state
address-space metadata
```

At that point, this temporary fixed scheduler can be replaced by a design that matches the virtual-memory model.
