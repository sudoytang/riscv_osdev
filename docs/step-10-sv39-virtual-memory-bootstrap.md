# Step 10: Sv39 Virtual Memory Bootstrap

The tenth milestone enables Sv39 page-based virtual memory for the kernel.

After this step, all CPU memory accesses go through the MMU. The kernel continues to run at the same addresses as before, but those addresses are now virtual addresses translated by a page table rather than raw physical addresses passed directly to the bus.

## Why Now

Step 9 warned that the temporary task scheduler was only acceptable before paging, because every task assumption it made depended on the absence of address space separation.

Before virtual memory can be revisited, the kernel needs a working MMU foundation:

```text
a kernel page table
paging enabled at boot
kernel code and data mapped and executable in S-mode
MMIO accessible after paging is on
```

This step provides exactly that foundation. It does not yet redesign the task model. That comes after.

## Sv39 Overview

Sv39 is the 39-bit virtual address mode defined by the RISC-V privileged specification.

A virtual address is split into three 9-bit indices and a 12-bit page offset:

```text
[38:30]  VPN[2]   indexes the L2 root page table (512 entries)
[29:21]  VPN[1]   indexes an L1 page table
[20:12]  VPN[0]   indexes an L0 page table
[11:0]   offset   byte offset within the 4096-byte page
```

Each page table entry (PTE) is 8 bytes and carries both a physical page number and a set of permission and status flags:

```text
[53:10]  PPN      physical page number (44 bits)
[9:8]    RSW      reserved for software
[7]      D        dirty: page has been written
[6]      A        accessed: page has been read or executed
[5]      G        global: entry is shared across all address spaces
[4]      U        user: page is accessible from U-mode
[3]      X        execute permission
[2]      W        write permission
[1]      R        read permission
[0]      V        valid: entry is present
```

If R, W, and X are all zero in a PTE, the entry is a pointer to the next level page table. If at least one of R, W, or X is set, the entry is a leaf that directly describes a physical page or superpage.

## The PageTable Struct

The new `src/mm.rs` module defines:

```rust
#[repr(align(4096))]
struct PageTable {
    entries: [usize; 512],
}
```

`512 * 8 = 4096` bytes exactly fills one physical page. The `#[repr(align(4096))]` attribute ensures the compiler places the static variable on a 4096-byte boundary, which the hardware requires.

The single root page table is a `static mut`:

```rust
static mut KERNEL_ROOT_PAGE_TABLE: PageTable = PageTable::new();
```

Because the kernel uses identity mapping, the compiler-assigned address of this static is also its physical address. The satp register is written with that physical address as the PPN.

## Why 1GB Gigapages

The full Sv39 page walk is three levels deep. Mapping the kernel with 4KB pages would require allocating L1 and L0 page tables in addition to the L2 root. That requires a physical memory allocator, which does not exist yet.

Sv39 supports a shortcut: if a leaf PTE is placed at the L2 level, it maps an entire 1GB region directly. Such an entry is called a gigapage or superpage.

Two gigapage entries are enough to cover everything the early kernel needs:

```text
root_page_table[0]  maps 0x00000000..0x3FFFFFFF  covers UART at 0x10000000
root_page_table[2]  maps 0x80000000..0xBFFFFFFF  covers kernel at 0x80200000
```

No L1 or L0 page tables are needed. No allocator is needed. The only memory required is the single static `KERNEL_ROOT_PAGE_TABLE` that is already reserved at compile time.

## Identity Mapping

Both gigapages use identity mapping: virtual address equals physical address.

This is the only viable approach for a seamless transition. When `csrw satp` enables paging, the CPU immediately begins translating the very next instruction fetch. If the virtual address of that instruction did not map to the same physical address, the CPU would fetch the wrong instruction and crash.

With identity mapping, the transition is invisible to the instruction stream. The program counter holds `0x8020xxxx` before and after paging is enabled, and the page table translates `0x8020xxxx` to the same `0x8020xxxx` in physical memory.

## The PTE Value Formula

A gigapage PTE is constructed as:

```text
PTE = (physical_address >> 12 << 10) | flags
```

Breaking that down:

```text
physical_address >> 12    extracts the physical page number (PPN)
<< 10                     places the PPN into PTE[53:10]
| flags                   sets the permission and status bits
```

For the kernel region at `0x80000000`:

```rust
((0x80000000_usize >> 12) << 10) | PTE_V | PTE_R | PTE_W | PTE_X | PTE_A | PTE_D
```

The A and D bits must be set manually. Some RISC-V implementations update A and D in hardware when pages are accessed or written. The QEMU virt machine instead raises a page fault when A or D is clear, expecting software to set the bits. Since the kernel has no handler for that class of fault at this stage, the bits are pre-set to avoid the fault entirely.

## The Two Mapped Regions

```rust
// 0x00000000..0x3FFFFFFF: R+W only, covers UART MMIO at 0x10000000
KERNEL_ROOT_PAGE_TABLE.entries[0] = ((0x0_usize >> 12) << 10) |
    PTE_V | PTE_R | PTE_W | PTE_A | PTE_D;

// 0x80000000..0xBFFFFFFF: R+W+X, covers kernel code and data
KERNEL_ROOT_PAGE_TABLE.entries[2] = ((0x80000000_usize >> 12) << 10) |
    PTE_V | PTE_R | PTE_W | PTE_X | PTE_A | PTE_D;
```

The UART region does not have the X bit. UART is memory-mapped I/O, not executable code. Setting X on it would be incorrect.

## Writing satp

The `satp` CSR controls the page table base and the address translation mode. Its layout is:

```text
[63:60]  MODE   8 = Sv39
[59:44]  ASID   address space identifier, 0 for now
[43:0]   PPN    physical page number of the root page table
```

```rust
pub fn enable_paging() {
    unsafe {
        let root_page_table_addr = core::ptr::addr_of!(KERNEL_ROOT_PAGE_TABLE) as usize;
        let satp = (8usize << 60) | (root_page_table_addr >> 12);
        core::arch::asm!(
            "csrw satp, {satp}",
            "sfence.vma",
            satp = in(reg) satp,
        )
    }
}
```

`sfence.vma` flushes all TLB entries. The TLB caches recent address translations. After changing satp, any cached translations from the previous mode are stale and must be discarded before the CPU resumes normal instruction fetching.

## Initialization Order

The trap handler must be initialized before paging is enabled. This is a hard requirement, not a style preference.

Between `csrw satp` and the first print that follows, the CPU is in a window where any interrupt or exception will jump to `stvec`. If `stvec` is not yet set, the CPU jumps to whatever happens to be at address zero.

In this kernel, zero is inside the 0x00000000..0x3FFFFFFF mapped region, which has R+W permissions but not X. An instruction fetch from that address causes an Instruction Page Fault. Because no trap handler is ready, the fault escalates to M-mode and OpenSBI terminates the machine.

The same window applies to `sscratch`. The trap entry code checks whether `sscratch` is zero to determine whether a trap came from S-mode or U-mode. If `sscratch` holds a leftover value from OpenSBI, the trap handler will misidentify an S-mode trap as a U-mode trap, use the OpenSBI value as a stack pointer, write into protected memory, and trigger an access fault that is not delegated to S-mode.

The correct initialization order is:

```rust
trap::init();             // csrw stvec: set trap vector
init_kernel_trap_stack(); // csrw sscratch, zero: establish S-mode convention
mm::map_kernel_gigapages();
mm::enable_paging();
```

This ordering was discovered through a real failure during this step. The first attempt placed `enable_paging` before `trap::init`, and the system appeared to work by chance: no interrupt happened to fire in the gap. A later change caused a timer interrupt to fall inside the gap, the trap handler misidentified it as coming from U-mode, and OpenSBI terminated the machine with no output. Moving `init_kernel_trap_stack` before `enable_paging` closed the window.

## The PTE_U Constraint

An early attempt added `PTE_U` to the kernel gigapage mapping to allow user tasks to execute kernel code. This caused an immediate crash.

The RISC-V privileged specification states:

> S-mode can never execute instructions from user pages, regardless of the state of SUM.

Setting `PTE_U` on a page permanently prevents S-mode from fetching instructions from that page. As soon as paging was enabled with `PTE_U` set on the kernel region, every subsequent instruction fetch in S-mode caused an Instruction Page Fault.

The reverse is also true. Without `PTE_U`, U-mode cannot execute the kernel code that currently hosts the user task entry points.

There is no single gigapage configuration that satisfies both constraints at once. A page that S-mode can execute cannot have `PTE_U` set. A page that U-mode can execute must have `PTE_U` set and therefore cannot be executed from S-mode.

This reveals a real architectural requirement. User task code must live in its own virtual address range, separate from the kernel, mapped with `PTE_U` in a per-task page table. User tasks are disabled for now:

```rust
// User tasks require per-task page tables with PTE_U.
// The current single shared mapping cannot satisfy both S-mode and U-mode
// execute permissions. Scheduling is disabled until that is addressed.
// task::schedule();
loop {}
```

## The SUM Bit

The SUM (Supervisor User Memory access) bit in `sstatus` controls whether S-mode can read and write pages marked with `PTE_U`.

```text
SUM=0 (default)  S-mode load/store to PTE_U pages faults
SUM=1            S-mode load/store to PTE_U pages succeeds
```

SUM affects only loads and stores, not instruction fetches. S-mode can never execute from a page with `PTE_U` set, regardless of SUM.

Once user tasks have their own address spaces, the kernel will need SUM to safely copy data to and from user memory. `SYS_WRITE` currently reads a user-provided pointer directly, which is only safe because no address space separation exists yet. A future redesign will use SUM with explicit bounds checking to replace that direct access.

## What This Step Proves

1. The kernel can build a minimal Sv39 page table at boot without a memory allocator.
2. The kernel can enable Sv39 paging and continue executing without any change to observable behavior.
3. UART MMIO remains accessible after paging is enabled.
4. Trap handling continues to work correctly under paging.
5. S-mode cannot execute from pages marked PTE_U, which enforces a hard boundary between kernel and user code pages.
6. Initialization order matters: both stvec and sscratch must be set before enabling paging.

## Next Step

The next step is to give each user task its own page table.

The per-task page table will need to map:

```text
user code pages     PTE_U | PTE_R | PTE_X
user stack pages    PTE_U | PTE_R | PTE_W
kernel mapping      shared, no PTE_U
```

With separate page tables, the kernel switches satp when entering a user task, giving each task an isolated virtual address space. User code is no longer required to share the same virtual addresses as kernel code.

After that, `SYS_WRITE` can be updated to validate the user pointer and use SUM for the controlled kernel access to user memory. The task structure will also grow to carry the page table root and saved context needed for a real context switch.
