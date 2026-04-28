# Step 8: Minimal Task Abstraction

The eighth milestone introduces the first task abstraction.

Before this step, the kernel entered user mode directly:

```rust
enter_user_mode(user_main as *const () as usize, user_stack_top());
```

That proved that U-mode, trap handling, and syscalls worked, but the kernel was not yet modeling a user program as something it owns and manages.

This step wraps the user program in a `Task`.

## Task State

The new `src/task.rs` module defines a small task state machine:

```rust
pub enum TaskState {
    Ready,
    Running,
    Exited,
}
```

For now, there is only one task. Even so, the states are useful because they describe the lifecycle we want the kernel to manage:

```text
Ready    the task exists but has not entered user mode yet
Running  the task is currently executing in U-mode
Exited   the task called SYS_EXIT
```

This is the first step toward a scheduler.

## Task Layout

The `Task` struct currently stores only the information needed to enter user mode:

```rust
pub struct Task {
    pub entry: usize,
    pub user_sp: usize,
    pub state: TaskState,
}
```

`entry` is the user program counter that will be written into `sepc`.

`user_sp` is the user stack pointer that will be moved into `sp` before `sret`.

Both are stored as `usize` because they are raw machine addresses used by CPU registers, not Rust references that the kernel should dereference through a typed pointer.

## Running the First Task

`kernel_main` now creates a task before entering user mode:

```rust
let mut task = task::Task::new(user_main as *const () as usize, user_stack_top());
run_task(&mut task);
```

The new `run_task` helper centralizes the transition:

```rust
fn run_task(task: &mut task::Task) -> ! {
    task::set_current_task(task);
    task::mark_current_task_state(task::TaskState::Running);
    krnl_println!("Entering user mode...");
    enter_user_mode(task.entry, task.user_sp);
}
```

The behavior is still the same from QEMU's point of view: the kernel enters the same `user_main` function with the same user stack.

The difference is architectural. The kernel now says:

```text
create a task
run the task
```

instead of:

```text
manually jump to this raw entry address and stack pointer
```

## Current Task Pointer

`SYS_EXIT` is handled inside the trap handler, not inside `kernel_main`.

That means the syscall path needs a way to find the task that is currently running. This step adds a single global current-task pointer:

```rust
static CURRENT_TASK: AtomicPtr<Task> = AtomicPtr::new(core::ptr::null_mut());
```

The kernel sets it before entering user mode:

```rust
task::set_current_task(task);
```

This is still a single-task design. It is not a full scheduler data structure yet. It only gives trap handling a way to update the current task's state.

`AtomicPtr` comes from `core::sync::atomic`, so it works in a `no_std` kernel. On this target, `riscv64gc` includes the atomic extension, so pointer-sized atomic operations are available.

## Moving SYS_EXIT Out of Trap Handling

Previously, `trap.rs` handled `SYS_EXIT` directly:

```rust
Syscall::Exit => {
    let exit_code = tf.regs[A0];
    krnl_println!("  User exited with code {}", exit_code);
    krnl_println!("  Kernel halted");
    loop {}
}
```

That mixed two responsibilities:

```text
trap.rs  decoding and dispatching traps
task.rs  managing task lifecycle
```

Now `trap.rs` only dispatches:

```rust
Syscall::Exit => {
    let exit_code = tf.regs[A0];
    crate::task::exit_current_task(exit_code);
}
```

The task module owns the task lifecycle transition:

```rust
pub fn exit_current_task(exit_code: usize) -> ! {
    mark_current_task_state(TaskState::Exited);

    crate::krnl_println!("  User task exited with code {}", exit_code);

    schedule();
}
```

This is a small but important separation. The trap handler no longer decides what the kernel should do after a task exits. It hands that decision to the task layer.

## Minimal Scheduler Stub

The scheduler is still only a stub:

```rust
fn schedule() -> ! {
    crate::krnl_println!("  No runnable tasks, kernel halted");
    loop {}
}
```

For a single task, this is correct enough:

```text
the only task exited
there are no other runnable tasks
halt the kernel
```

Later, this function will become the place where the kernel searches for another ready task.

## Why run_task Still Returns Never

`run_task` still returns `!`:

```rust
fn run_task(task: &mut task::Task) -> !
```

This is because `enter_user_mode` enters user mode with `sret`, and the current exit path does not return to the original Rust call stack.

When the user program calls `SYS_EXIT`, the CPU enters the trap handler on the kernel trap stack. From there, the kernel marks the task as exited and calls `schedule`.

Returning to the original `run_task` caller would require saving and restoring a kernel context, similar to a context switch. That is a later step.

## What This Step Proves

This step proves that:

1. The kernel can represent a user program as a `Task`.
2. The task has a visible lifecycle: `Ready -> Running -> Exited`.
3. The syscall path can update kernel task state.
4. `trap.rs` no longer owns the exit policy after `SYS_EXIT`.
5. A scheduler entry point now exists, even though it only halts for now.

## Next Step

The next major step is to make `schedule` choose work instead of always halting.

There are two natural directions:

```text
multiple static tasks
kernel context save/restore
```

The simpler next milestone is multiple static tasks. That would let the kernel create two user tasks and eventually run them one at a time.

The deeper milestone is context switching. That would let the kernel save where it was, leave user mode, enter the scheduler, and later resume another task.
