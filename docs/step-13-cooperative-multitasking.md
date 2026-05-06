# Step 13: Cooperative Multitasking

The thirteenth milestone introduces context switching and cooperative multitasking. Tasks can now yield the CPU voluntarily and be resumed later, giving each task an independent kernel execution context and an isolated virtual address space.

## What Changed

- `TaskContext` and `switch_context`: the kernel-side context switch primitive
- Per-task kernel stacks: each task owns a 4KB kernel stack allocated from the frame allocator
- `SCHEDULER_CONTEXT` and the scheduler loop: a permanent boot-thread that acts as the hub for all task switching
- `sched()`: the internal function that switches from a task back to the scheduler
- `SYS_YIELD` syscall: the user-facing interface for voluntary preemption
- User syscall module: user-space syscall wrappers moved to `user/src/syscall.rs`

## Two Layers of Context

A running task has two distinct execution contexts that are saved and restored independently:

```
TrapFrame    U-mode context saved by __trap_entry at the moment of ecall/interrupt
TaskContext  S-mode kernel context saved by switch_context at the deepest call frame
```

`TrapFrame` captures the state of user-mode execution: all 32 general-purpose registers, `sepc`, `sstatus`, `scause`, and `stval`. It is stored on the task's kernel stack and managed by the trap entry/exit assembly.

`TaskContext` captures the state of the kernel execution chain: only the 14 callee-saved registers (`ra`, `sp`, `s0`–`s11`). It is stored in the `Task` struct and managed by `switch_context`.

### Why TaskContext only needs callee-saved registers

`switch_context` is called like a normal function, so the RISC-V calling convention already protects the caller-saved registers. By the time `switch_context` is called, the compiler has either spilled caller-saved values onto the stack or discarded them. The kernel stack preserves those spilled values across the switch; only the callee-saved registers need to be captured explicitly in `TaskContext`.

This would break for preemptive scheduling, where a timer interrupt can fire at any instruction boundary before any calling convention has had a chance to save caller-saved registers. Preemptive context switches must use `TrapFrame` instead, which saves all 32 registers at the trap boundary.

## Per-task Kernel Stacks

When a task yields via `SYS_YIELD`, the kernel call chain is not unwound. It is frozen in place on the task's kernel stack:

```
kernel_stack_top
  TrapFrame              (saved by __trap_entry)
  trap_handler frame
  handle_syscall frame
  yield_current_task frame
  sched frame
  switch_context frame   ← TaskContext.sp points here
```

The entire chain must be preserved intact so that when the task is resumed, `switch_context` can return and the chain can unwind normally back through the trap exit path and `sret` to user mode.

This is why each task needs its own kernel stack. If two tasks shared a stack, one task resuming would overwrite the other's frozen call chain.

## The Scheduler as a Boot Thread

`schedule()` runs on the original boot stack and never exits. It is not scheduled by anyone; it simply loops, picking the next Ready task and switching to it. All task context switches pass through it:

```
boot stack (schedule() loop)
│
│  switch_context → task 0 kernel stack
│                   task_first_run → enter_user_mode → sret
│                   [U-mode running]
│                   ecall → sched()
│  switch_context ←──────────────────
│
│  switch_context → task 1 kernel stack
│                   ...
│  switch_context ←──────────────────
│
│  switch_context → task 0 kernel stack (resumes frozen call chain)
│                   sched returns → yield_current_task returns → ...
│                   → sret back to U-mode
│  switch_context ←──────────────────
```

`SCHEDULER_CONTEXT` holds the saved state of `schedule()` itself (the `ra` and `sp` of the call site where `switch_context` was last called). When a task calls `sched()`, the `switch_context` there loads `SCHEDULER_CONTEXT` and returns inside `schedule()`'s loop, which then picks the next task.

This design concentrates all scheduling policy in one place (`schedule()`) rather than scattering it across yield/exit paths. It mirrors the xv6 design where every transition goes through the per-CPU scheduler context.

## switch_context

```rust
#[unsafe(naked)]
unsafe extern "C" fn switch_context(current: *mut TaskContext, next: *mut TaskContext) {
    // Save ra, sp, s0-s11 to *current
    // Load ra, sp, s0-s11 from *next
    // ret  (jumps to next.ra)
}
```

`ret` is the pseudoinstruction for `jalr x0, ra, 0`. After loading `next.ra` into `ra`, `ret` transfers execution to that address. For a task's first run this is `task_first_run`; for a resumed task it is the instruction after the `switch_context` call inside `sched()`.

## task_first_run

A new task's `TaskContext` is initialised with `ra = task_first_run` and `sp = kernel_stack_top`. When `schedule()` calls `switch_context` for the first time, `ret` jumps to `task_first_run` with the task's empty kernel stack already in place. This function reads the current task's `entry`, `user_sp`, and `kernel_stack_top`, then calls `enter_user_mode` which sets `sscratch` to `kernel_stack_top` and `sret`s into user code.

## sscratch and saved_user_sp

`sscratch` is a global CSR. Its value must be correct for whichever task is currently running:

- While a task is in U-mode: `sscratch` = that task's `kernel_stack_top`, so a trap can swap `sp ↔ sscratch` and land on the right kernel stack
- While in the trap handler: `sscratch` = that task's user `sp`, saved there by the trap entry swap

`saved_user_sp` in the `Task` struct exists because `sscratch` is overwritten when the scheduler switches to a different task. Before calling `switch_context`, `sched()` reads `sscratch` and stores the value in `task.saved_user_sp`. When `schedule()` later resumes this task, it writes `saved_user_sp` back into `sscratch` before calling `switch_context`, so the trap epilogue finds the correct user `sp` when it executes `csrrw sp, sscratch, sp`.

## SYS_YIELD flow

```
user ecall SYS_YIELD
  __trap_entry: swap sp↔sscratch, save TrapFrame on kernel stack
  trap_handler → handle_syscall(Yield, tf)
    yield_current_task()
      task.state = Ready
      sched()
        task.saved_user_sp = sscratch   (= user sp)
        switch_context(&task.ctx, &SCHEDULER_CONTEXT)
          → schedule() loop resumes
          → finds next Ready task, switch_context to it
        [... time passes ...]
        switch_context returns (scheduler picked this task again)
      sched() returns
    yield_current_task() returns
  handle_syscall() returns
  tf.sepc += 4                          (advance past ecall)
  trap_handler returns to __trap_entry
  sscratch restored to user sp by schedule()
  __trap_entry: restore TrapFrame, csrrw sp↔sscratch, sret
user resumes after ecall
```

## Floating-point registers

The RISC-V calling convention lists `fs0`–`fs11` as callee-saved, but `TaskContext` does not include them. The kernel generates no floating-point instructions, and `sstatus.FS` is `Off` by default (any FP instruction would fault). When user programs are allowed to use floating-point, all 32 FP registers will need to be saved and restored at context switch time, typically with a lazy save/restore optimisation.

## Halting with wfi

The current `loop {}` at the end of `schedule()` burns 100% CPU while waiting. The correct implementation is:

```rust
loop {
    unsafe { core::arch::asm!("wfi"); }
}
```

`wfi` (Wait For Interrupt) suspends the hart until an interrupt arrives, consuming negligible power. Once timer interrupts and I/O interrupts are wired up, the idle loop will wake on each interrupt, check whether any blocked task has become Ready, and schedule it. The `loop {}` is a placeholder until that infrastructure exists.

## What This Step Proves

1. Two tasks interleave correctly across voluntary yields.
2. Each task's kernel call chain is preserved across a yield and correctly resumed.
3. The scheduler loop correctly implements round-robin: after task N yields, task N+1 runs next.
4. Per-task kernel stacks are allocated on task creation and freed (via `schedule()`) after exit.
5. `sscratch` is correctly maintained across task switches so every trap lands on the right kernel stack.

## Next Step

The next step is to add timer interrupts and preemptive scheduling. This requires:

- Programming `mtimecmp` via an SBI call to schedule a timer interrupt
- Handling `SupervisorTimer` in `trap_handler` by calling into the scheduler
- Saving and restoring all 32 registers (via `TrapFrame`) instead of only callee-saved registers, since a timer interrupt can arrive at any instruction boundary
