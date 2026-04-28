# Step 5: Entering U-mode and Printing through Syscalls

The fifth milestone is the first real user/kernel boundary in this project.

Before this step, the kernel could handle traps and return from a deliberate illegal instruction. However, all code still ran in S-mode. In this step, the kernel enters U-mode, runs a small user function, handles `ecall` traps from user mode, and lets user code print through a syscall instead of touching the UART directly.

The high-level flow is:

```text
S-mode kernel
  -> initialize stvec
  -> clear sscratch while running in kernel mode
  -> prepare user stack and kernel trap stack
  -> set sepc to user_main
  -> set sstatus.SPP = 0
  -> set sscratch = kernel_trap_stack_top
  -> set sp = user_stack_top
  -> sret into U-mode

U-mode user_main
  -> user_println!
  -> SYS_PUTCHAR ecall for each byte

S-mode trap handler
  -> switch from user stack to kernel trap stack
  -> save TrapFrame
  -> decode EnvironmentCallFromUMode
  -> handle Syscall::PutChar
  -> restore context
  -> sret back to U-mode
```

## Kernel and User Output

The UART output path was renamed to make the privilege boundary clearer:

```rust
krnl_print!(...)
krnl_println!(...)
```

These macros are kernel-only. They write to the UART through MMIO and should not be used by user-mode code.

User code now uses a separate printing path:

```rust
user_print!(...)
user_println!(...)
```

These macros format text in user code, but each byte is sent through an `ecall`:

```rust
fn user_putchar(ch: u8) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") trap::Syscall::SYS_PUTCHAR,
            in("a0") ch as usize,
        );
    }
}
```

This keeps the conceptual model correct:

```text
user mode does not write UART directly
user mode asks the kernel to write
kernel mode performs the UART MMIO access
```

Right now there is no virtual memory, so U-mode may still be able to access more than it should. The naming split is still useful because it prevents the source code from depending on that temporary lack of isolation.

## User and Kernel Trap Stacks

Two separate stacks are prepared:

```rust
static mut USER_STACK: Stack = Stack::new();
static mut KERNEL_TRAP_STACK: Stack = Stack::new();
```

`USER_STACK` is used while executing U-mode code.

`KERNEL_TRAP_STACK` is used when a U-mode trap enters the S-mode kernel.

This matters because RISC-V does not automatically switch stacks on a trap. When U-mode executes `ecall`, the CPU enters S-mode, but the `sp` register still contains the user stack pointer. If the kernel saved its `TrapFrame` there, user code would control the memory used by the kernel trap handler.

So the kernel uses `sscratch` as a stack-switching aid.

The convention is:

```text
while running in S-mode kernel code:
  sscratch = 0

before entering U-mode:
  sscratch = kernel_trap_stack_top
```

Then the trap entry begins with:

```asm
csrrw sp, sscratch, sp
```

For an S-mode trap:

```text
before:
  sp       = kernel sp
  sscratch = 0

after csrrw:
  sp       = 0
  sscratch = original kernel sp
```

The trap entry sees `sp == 0`, swaps back, and handles the trap on the current kernel stack.

For a U-mode trap:

```text
before:
  sp       = user sp
  sscratch = kernel_trap_stack_top

after csrrw:
  sp       = kernel_trap_stack_top
  sscratch = user sp
```

The trap entry sees `sp != 0`, keeps the new kernel stack, and saves the trap frame there.

## Entering User Mode

The kernel enters user mode with `enter_user_mode`:

```rust
fn enter_user_mode(entry: usize, user_sp: usize) -> ! {
    unsafe {
        core::arch::asm!(
            "csrw sepc, {entry}",
            "csrw sscratch, {kernel_sp}",
            "csrr t0, sstatus",
            "li t1, {sstatus_spp}",
            "not t1, t1",
            "and t0, t0, t1",
            "li t1, {sstatus_spie}",
            "or t0, t0, t1",
            "csrw sstatus, t0",
            "mv sp, {user_sp}",
            "sret",
            // operands omitted
        )
    }
}
```

The key parts are:

```text
sepc = user_main
sscratch = kernel_trap_stack_top
sstatus.SPP = 0
sp = user_stack_top
sret
```

`sstatus.SPP` controls which privilege mode `sret` returns to:

```text
SPP = 0 -> return to U-mode
SPP = 1 -> return to S-mode
```

Clearing `SPP` makes `sret` enter U-mode at `user_main`.

## Syscall Type

Syscalls are represented by a small enum:

```rust
pub enum Syscall {
    Reserved,
    PutChar,
    Unknown(usize),
}
```

Currently only one real syscall is implemented:

```rust
SYS_PUTCHAR = 1
```

`a7` carries the syscall number, and `a0` carries the first argument:

```text
a7 = SYS_PUTCHAR
a0 = byte to print
```

The kernel decodes the syscall number:

```rust
let syscall_num = tf.regs[A7];
let syscall = Syscall::from_number(syscall_num);
syscall.krnl_handle(tf);
```

Then `Syscall::PutChar` writes the byte through the kernel UART path:

```rust
let ch = tf.regs[A0] as u8;
krnl_print!("{}", ch as char);
tf.regs[A0] = 0;
```

The return value is placed back in `a0` by modifying `tf.regs[A0]`.

## Quiet Syscall Traps

Normal syscalls happen often. `user_println!` currently invokes `SYS_PUTCHAR` once per byte. If the trap handler printed full trap diagnostics for every syscall, user output would be unreadable.

So normal U-mode ecall traps are quiet by default:

```rust
const TRACE_SYSCALLS: bool = false;
```

Unexpected traps still print diagnostic information.

## What This Step Proves

This step proves that:

1. The kernel can enter U-mode with `sret`.
2. U-mode code can execute on a separate user stack.
3. U-mode `ecall` traps into the S-mode kernel.
4. The trap entry can switch from the user stack to a kernel trap stack.
5. The kernel can decode a syscall number from `a7`.
6. The kernel can return syscall results through `a0`.
7. User-mode printing can go through the syscall path.

This is the first working user/kernel API in the project.

## Next Step

The next useful syscall is:

```text
SYS_EXIT
```

Right now `user_main` ends with:

```rust
loop {}
```

With `SYS_EXIT`, user code can terminate explicitly:

```rust
user_exit(0);
```

The kernel can then report the exit code and stop or schedule another user task later.
