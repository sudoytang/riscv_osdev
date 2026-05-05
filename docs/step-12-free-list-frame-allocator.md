# Step 12: Free-list Frame Allocator

The twelfth milestone replaces the bump frame allocator with a free-list allocator that supports both allocation and deallocation, and adds proper cleanup when a user task exits.

## The Problem With the Bump Allocator

The bump allocator from the previous step advanced a pointer forward on every `alloc_frame` call and had no `free_frame`. Every physical page handed out was permanently consumed. This was acceptable while tasks lived forever, but becomes a memory leak the moment tasks are allowed to exit and their resources need to be reclaimed.

## Intrusive Free List

The replacement allocator is an intrusive singly-linked list of free 4KB pages. No separate metadata structure is needed. Each free page stores the address of the next free page in its own first eight bytes. When the page is allocated, those bytes are overwritten by the caller anyway.

```
FREE_LIST_HEAD ──► [page A: next=B | ...] ──► [page B: next=C | ...] ──► [page C: next=0 | ...]
```

The global state is a single pointer:

```rust
static mut FREE_LIST_HEAD: usize = 0;
```

Zero means the list is empty.

### init_frame_allocator

```rust
pub fn init_frame_allocator(start: usize, end: usize)
```

Walks `[start, end)` in 4096-byte steps. For each page, writes the address of the next page into its first eight bytes. The last page gets zero. The head is set to `start`.

Initialization is O(n) in the number of pages. For 128MB of RAM above the kernel, that is roughly 32 000 pages, which takes negligible time at boot.

The call site in `main.rs` passes the full available range:

```rust
mm::init_frame_allocator(ekernel_addr, crate::PHYSICAL_MEMORY_END);
```

This change also makes the allocator aware of the upper bound, which the old bump allocator checked at allocation time. The free list has no upper-bound check at allocation time because the list simply becomes empty when all pages are consumed.

### alloc_frame

```rust
pub fn alloc_frame() -> usize
```

Pops the head page from the list, advances the head to the next pointer stored in that page, zero-initializes the page, and returns its address.

The return type changed from `*mut u8` to `usize`. The address is a physical address. Callers that need a typed pointer cast it themselves (`addr as *mut PageTable`). This makes the physical-address nature of the return value explicit at the call site.

### free_frame

```rust
pub fn free_frame(addr: usize)
```

Pushes a page back onto the list head. Writes the current head into the first eight bytes of the page, then sets the head to this page. O(1).

## Task Cleanup on Exit

With `free_frame` available, `exit_current_task` can now reclaim all memory belonging to an exiting task.

### What needs to be freed

A user task owns:

- The root L2 page table (1 frame, allocated by `create_user_page_table`)
- One L1 page table for VPN[2]=1 (1 frame, allocated by `map_page`)
- One L0 page table for VPN[1]=0 (1 frame, allocated by `map_page`)
- The user stack page (1 frame, allocated by `init_task`)

The user code pages are not owned by the task. They map into the `.user_bin` section that is embedded in the kernel ELF at compile time. That memory is never dynamically allocated and must not be freed.

### free_user_page_table

```rust
pub fn free_user_page_table(root_pa: usize)
```

Walks the three-level page table tree and frees everything that was dynamically allocated:

1. For each valid non-kernel L2 PTE (skipping index 0 and 2 which are kernel gigapages), extract the L1 page table address.
2. For each valid L1 PTE, extract the L0 page table address.
3. For each valid L0 leaf PTE, extract the mapped physical frame address. Free it only if it falls within the dynamically allocated range `[ekernel, PHYSICAL_MEMORY_END)`. This check skips the user code pages that map into the kernel ELF.
4. Free the L0 page table itself.
5. After all L1 entries are processed, free the L1 page table itself.
6. After all L2 entries are processed, free the root page table itself.

The range check `[ekernel, PHYSICAL_MEMORY_END)` is the boundary between the static kernel image and the dynamically allocated physical memory. Anything below `ekernel` is part of the kernel binary and must not be reclaimed.

### Deallocation Order and satp

The deallocation order in `exit_current_task` matters:

```rust
crate::mm::switch_to_kernel_page_table(); // 1. switch satp back to kernel page table
crate::mm::free_user_page_table(...);     // 2. now safe to free the user page table
```

If the order were reversed, `free_user_page_table` would be walking page table memory while `satp` still points at it. `free_frame` writes a list pointer into the first eight bytes of each freed page. That write would corrupt the very page table entries being walked. Switching `satp` first ensures the CPU no longer depends on the user page table for address translation before any of its pages are recycled.

`switch_to_kernel_page_table` calls `enable_paging`, which includes `sfence.vma`. This flushes the TLB, removing any cached translations from the user address space before the pages are returned to the free list.

### Module Structure

The allocator was moved to its own submodule `src/mm/freelist.rs` and re-exported from `src/mm.rs`:

```rust
pub mod freelist;
pub use freelist::{init_frame_allocator, alloc_frame, free_frame};
```

All allocator state (`FREE_LIST_HEAD`) is private to `freelist.rs`. The only way to interact with it is through the three exported functions.

## What This Step Proves

1. Physical frames can be allocated and reclaimed without any per-allocation metadata overhead.
2. A user task that exits no longer leaks its stack or page table memory.
3. The page table tree can be walked correctly to enumerate all dynamically allocated frames belonging to a task.
4. The deallocation sequence (switch satp, then free) avoids corrupting active page table structures.

## Next Step

The free list handles individual 4KB pages. The next natural extension is to support multi-page contiguous allocations, which are needed for larger kernel data structures. This leads toward a buddy system, where pages are grouped into power-of-two blocks and split or merged on demand.
