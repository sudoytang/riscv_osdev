# Step 14: Spawn and Program Image Store

The fourteenth milestone separates **program images** from **running task instances**. The embedded `user.bin` flat binary is treated as a read-only linear program repository. Each `spawn` copies the image into freshly allocated physical pages and creates an independent user address space.

## What Changed

- `ProgramImage` store in `src/program.rs`: read-only view of the embedded `user.bin`
- `ProgramEntry`: named entry points with byte offsets into the flat binary
- `create_user_address_space`: copies program bytes into private code pages instead of mapping the embedded copy directly
- `spawn`: allocates a task slot, user stack, kernel stack, and user page table for each new instance
- `schedule_until_idle`: scheduler returns when no Ready tasks remain, allowing the kernel to spawn again
- Repeat-run validation in `kernel_main`: after initial tasks exit, `user_main_0` is spawned a second time
- **Build fix**: `build.rs` now reliably rebuilds and re-embeds `user.bin` when user sources change
- **Tagged log output**: kernel and user prints are prefixed with their source function so QEMU output is easy to trace
- `user/.gitignore`: ignores `user/target/` (separate from the root `/target` ignore)

## Program Image vs Task Instance

```
Embedded user.bin (read-only, in kernel .user_bin section)
        |
        | spawn copies image
        v
Private user code pages (allocated per task)
        +
Private user stack page
        +
Per-task Sv39 page table
        =
One runnable task instance
```

When a task exits, its private code pages, user stack, page table, and kernel stack are freed. The original embedded `user.bin` remains available for the next `spawn`.

## ProgramEntry

The current user crate still contains two entry points in one flat binary:

```asm
_start:
    j user_main_0    ; offset 0
    j user_main_1    ; offset 4
```

The kernel describes these as:

```rust
pub const USER_MAIN_0: ProgramEntry = ProgramEntry {
    name: "user_main_0",
    entry_offset: 0,
};

pub const USER_MAIN_1: ProgramEntry = ProgramEntry {
    name: "user_main_1",
    entry_offset: 4,
};
```

User virtual addresses:

- Code base: `0x40000000`
- Stack base: `0x40010000` (one 4KB page, stack pointer at top)

## spawn Flow

1. Find a free task slot (`Unused` or `Exited`).
2. Read the embedded program image via `embedded_user_bin()`.
3. Allocate a user stack page and kernel stack page.
4. Call `create_user_address_space` to copy the image and build a page table.
5. Initialize `TaskContext` with `ra = task_first_run` and `sp = kernel_stack_top`.
6. Mark the task `Ready` for the scheduler.

## Exit and Re-spawn

On `SYS_EXIT`:

1. Mark the task `Exited`.
2. Switch back to the kernel page table.
3. Free the user page table and all private mapped frames.
4. Return to the scheduler, which frees the kernel stack on the next iteration.

`kernel_main` validation flow:

1. Spawn `user_main_0`, then `user_main_1` twice (two independent instances of the same entry).
2. Run `schedule_until_idle()` until all three tasks exit.
3. Spawn `user_main_0` again to confirm a exited slot and program image can be reused.
4. Run `schedule_until_idle()` again, then halt.

Because exited slots can be reused, the kernel can call `spawn` again with the same `ProgramEntry` and the program will start from the beginning in a fresh address space.

## Rebuilding the Embedded User Binary

Editing `user/src` and running `cargo run` at the project root should pick up changes automatically. The root `build.rs` compiles the user crate into `OUT_DIR/user_target/`, converts the ELF to a flat `user.bin`, and embeds it into the kernel via `.incbin`.

A plain `cargo:rerun-if-changed=user/` watch is not always enough: `USER_BIN_PATH` stays the same across rebuilds, so Cargo may skip recompiling the kernel even after `user.bin` content changes. The fix:

1. Watch specific user inputs explicitly (`user/src`, `user/Cargo.toml`, `user/user.ld`, `user/build.rs`).
2. Hash the generated `user.bin` and export it as `USER_BIN_CHECKSUM`.
3. Reference `env!("USER_BIN_CHECKSUM")` in `src/main.rs` so the kernel crate rebuilds when the embedded binary changes.

```rust
// build.rs (after writing user.bin)
println!("cargo:rustc-env=USER_BIN_CHECKSUM={:016x}", checksum);

// src/main.rs
const _: &str = env!("USER_BIN_CHECKSUM");
```

**Note:** building inside `user/` writes to `user/target/`, which the main project does not use. Always rebuild from the project root with `cargo run`.

## Tagged Log Output

Kernel and user prints are prefixed with a tag naming the function that emitted them. This makes it easy to tell whether a line came from boot code, the scheduler, a syscall handler, or a specific user entry point.

| Prefix | Source |
|--------|--------|
| `[kernel_main]` | Boot and spawn orchestration in `kernel_main` |
| `[spawn]` | `spawn()` in `task.rs` |
| `[task_first_run]` | First switch into a new task |
| `[schedule_until_idle]` | Scheduler idle message |
| `[exit_current_task]` | Task exit |
| `[trap_handler]` | Trap dispatch |
| `[handle_syscall]` | Syscall handling |
| `[user.task_0]` | `user_main_0` |
| `[user.task_1]` | `user_main_1` |

Example QEMU excerpt:

```
[kernel_main] Spawning initial tasks...
[spawn]  spawn slot 0: user_main_0 @ 0x40000000
[spawn]  spawn slot 1: user_main_1 @ 0x40000004
[task_first_run] Entering user mode...
[user.task_0] Task 0
[user.task_0] Hello from U-mode via syscall!
[task_first_run] Entering user mode...
[user.task_1] Task 1
...
[exit_current_task]  User task exited with code 0
[schedule_until_idle]  No runnable tasks
[kernel_main] Re-spawning user_main_0 to validate repeat run...
```

## What This Step Proves

1. Program images and task instances are separate.
2. Each spawn gets its own copy of the user code in private physical pages.
3. A task can exit and the same program entry can be spawned again in the same kernel run.
4. The embedded `user.bin` acts as a minimal initramfs-style program store without a full filesystem.
5. Editing user sources and running `cargo run` at the project root rebuilds and re-embeds the user binary.

## Next Step

The next step is timer interrupts and preemptive scheduling:

- Program `mtimecmp` via SBI to raise timer interrupts
- Handle `SupervisorTimer` in `trap_handler`
- Preempt running tasks without requiring `SYS_YIELD`
- Use `wfi` in the idle loop once interrupts are wired up

After that, the program image store can evolve into a minimal initramfs/ramfs with named program lookup.
