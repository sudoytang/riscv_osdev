# Step 11: Per-task Page Tables and Separate User Binary

The eleventh milestone completes user-space isolation. Each user task now runs in its own virtual address space, user code lives in a separately compiled binary embedded into the kernel image, and the frame allocator provides the dynamic physical memory needed to build per-task page tables.

## What Changed

Four things happened in this step, and they are tightly coupled:

1. A bump-based physical frame allocator was added to `mm.rs`.
2. User code was moved to a separate crate compiled to a flat binary and embedded in the kernel via `.incbin`.
3. `create_user_page_table` builds an isolated Sv39 page table for each task.
4. `SYS_WRITE` uses the `SUM` bit to safely access user memory from S-mode.

## Why a Separate User Binary

The original approach placed user entry points (`user_main_0`, `user_main_1`) directly inside the kernel crate. This worked before virtual memory, when the kernel and user code shared a single address space.

After per-task page tables are introduced, user code must execute at virtual addresses in the user range (above `0x40000000`), not in the kernel range (`0x80200000`). Relocating kernel-linked functions to an arbitrary virtual address at runtime is not practical. The compiler emits PC-relative calls and the linker resolves them relative to the kernel base. Moving just a few functions to a different VA while keeping the rest of the kernel at the kernel base breaks every cross-function call.

The only clean solution is to compile user code as a completely independent binary, linked from scratch at `0x40000000`. That binary is then embedded in the kernel ELF at build time using a `.incbin` directive, and its physical address range is mapped into each task's page table.

```
kernel ELF
├── kernel code at 0x80200000
├── .user_bin section (raw flat binary)   ← embedded user binary lives here
└── boot stack + ekernel
```

At runtime, the kernel reads `user_bin_start` and `user_bin_end` to find the embedded binary in physical memory, then maps those physical pages into the user virtual range.

## The Build System

A `build.rs` in the kernel root drives the process:

1. Finds `llvm-objcopy` under the active Rust toolchain.
2. Runs `cargo build` inside the `user/` sub-crate.
3. Converts the resulting ELF to a flat binary with `llvm-objcopy -O binary`.
4. Exports the path to the flat binary as the `USER_BIN_PATH` environment variable.

The kernel's `main.rs` consumes `USER_BIN_PATH` at compile time:

```rust
core::arch::global_asm!(concat!(
    r#".section .user_bin, "a"
.align 12
.global user_bin_start
user_bin_start:
"#,
    ".incbin \"", env!("USER_BIN_PATH"), "\"\n",
    r#".align 12
.global user_bin_end
user_bin_end:
"#
));
```

The `user/` crate is a separate `no_std` binary with its own linker script (`user/user.ld`) that sets `BASE_ADDRESS = 0x40000000`. It also carries its own `.cargo/config.toml` that overrides `rustflags = []` to prevent the kernel's linker flags from being inherited.

## The Frame Allocator

Before per-task page tables can be built, the kernel needs a way to allocate physical pages at runtime. A bump allocator is the simplest possible implementation:

```rust
static mut FRAME_ALLOCATOR_NEXT_ADDR: usize = 0;

pub fn init_frame_allocator(start_addr: usize) { ... }

pub fn alloc_frame() -> *mut u8 { ... }
```

`init_frame_allocator` is called with the address of `ekernel`, the linker-defined symbol that marks the end of all kernel sections. Every subsequent allocation advances the pointer by 4096 bytes and zero-initializes the new page.

This allocator does not support `free`. All allocations it hands out live forever. That is acceptable for now because the only callers are `map_page` (for page table nodes) and `init_task` (for user stacks), and neither of those needs to be reclaimed during the current phase of development.

## Per-task Page Tables

`create_user_page_table` builds a fresh Sv39 root page table for a task:

```rust
pub fn create_user_page_table(user_stack_pa: usize) -> *mut PageTable {
    let root = alloc_frame() as *mut PageTable;
    // Copy kernel gigapage entries so the kernel is reachable from user vspace
    (*root).entries[0] = KERNEL_ROOT_PAGE_TABLE.entries[0];
    (*root).entries[2] = KERNEL_ROOT_PAGE_TABLE.entries[2];
    // Map user binary pages at 0x40000000
    for i in 0..num_pages {
        map_page(root, 0x40000000 + i * 4096, bin_start + i * 4096,
            PTE_V | PTE_U | PTE_R | PTE_X | PTE_A | PTE_D);
    }
    // Map user stack at 0x40010000
    map_page(root, 0x40010000, user_stack_pa,
        PTE_V | PTE_U | PTE_R | PTE_W | PTE_A | PTE_D);
    root
}
```

Key points:

- The kernel gigapage entries are copied verbatim from `KERNEL_ROOT_PAGE_TABLE`. This gives the kernel a valid mapping inside the user address space so it can continue executing after the `csrw satp` switch. The kernel entries do not carry `PTE_U`, so user code cannot reach kernel pages.
- User code pages carry `PTE_U | PTE_R | PTE_X`. User stack carries `PTE_U | PTE_R | PTE_W`. Neither has both R/W/X, which avoids write-execute aliasing.
- User code is mapped starting at `0x40000000`. The number of pages is computed from `user_bin_end - user_bin_start`, rounded up to the nearest 4KB boundary.
- User stack is mapped at `0x40010000`, separated from code by 64KB.

`init_task` now allocates its own stack frame and calls `create_user_page_table`:

```rust
pub fn init_task(id: usize) {
    let entry_va = 0x40000000 + id * 4; // word-offset into jump table
    let stack_pa = mm::alloc_frame() as usize;
    let user_sp_va = 0x40010000 + 4096; // top of 1-page stack
    let user_page_table_pa = mm::create_user_page_table(stack_pa) as usize;
    ...
}
```

When `run_task` is called, it switches `satp` to the per-task page table before entering user mode:

```rust
let satp = (8usize << 60) | (task.user_page_table_pa >> 12);
core::arch::asm!("csrw satp, {satp}", "sfence.vma", satp = in(reg) satp);
```

## The SUM Bit in SYS_WRITE

After per-task page tables are active, user data lives on pages marked `PTE_U`. S-mode cannot access `PTE_U` pages by default: any load or store from S-mode to a `PTE_U` page triggers a Load Page Fault or Store Page Fault.

`SYS_WRITE` copies data from a user pointer, which is on a `PTE_U` page. Without special handling, the kernel would fault the moment it dereferenced the pointer.

The fix is the `SUM` (Supervisor User Memory access) bit in `sstatus`, bit 18:

```text
SUM=0 (default)  S-mode load/store to PTE_U pages raises a page fault
SUM=1            S-mode load/store to PTE_U pages succeeds
SUM has no effect on instruction fetches; S-mode can never execute PTE_U pages
```

`SYS_WRITE` brackets the user memory access with atomic CSR instructions:

```rust
core::arch::asm!("csrs sstatus, {sum}", sum = in(reg) 1usize << 18);
let bytes = core::slice::from_raw_parts(ptr, size);
// ... read bytes ...
core::arch::asm!("csrc sstatus, {sum}", sum = in(reg) 1usize << 18);
```

`csrs` sets the specified bits in the CSR. `csrc` clears them. Both are atomic single-instruction operations. SUM is held for the minimum time needed and is cleared before returning to user mode.

SUM is kept narrow on purpose. Leaving SUM permanently set would allow any accidental kernel pointer dereference of a user address to succeed silently instead of faulting. The narrow window makes user pointer bugs easier to catch.

## The Nested Trap Failure

An early version of this step placed the user buffer read inside `handle_syscall` without setting SUM. The kernel faulted on the user pointer dereference (Load Page Fault). That fault re-entered the trap handler. At that point `sscratch` still held the user stack pointer, so the trap handler interpreted it as a U-mode trap and attempted to save the trap frame onto the user stack. Writing to the user stack also required SUM, which was still unset, producing a Store Page Fault at the trap save address. The second fault was what appeared in the QEMU log, not the original Load Page Fault that caused it.

Setting SUM before the user pointer dereference closed both failure modes.

## What This Step Proves

1. The kernel can compile and embed a separate user binary at build time.
2. Each user task has an isolated Sv39 virtual address space.
3. User code executes at `0x40000000`, fully separated from kernel code at `0x80200000`.
4. The kernel can safely access user memory in syscall handlers using the SUM bit.
5. The bump frame allocator correctly provides zeroed 4KB pages for page tables and user stacks.
6. Two user tasks run to completion independently and the kernel halts cleanly.

## Next Step

The bump allocator has no `free`. Every page allocated by `alloc_frame` is permanently consumed. This is sufficient for static structures like page tables, but any future feature that creates and destroys tasks will leak memory.

The next step is to replace the bump allocator with a free-list page allocator. Each free page stores a pointer to the next free page in its own first eight bytes. Initialization walks the physical memory range and links all pages into the list. `alloc_frame` pops from the list head. `free_frame` pushes back to the list head.

With `free_frame` in place, the kernel can reclaim user stack and page table pages when a task exits, which is the prerequisite for reusable task slots and eventually for process lifecycle management.
