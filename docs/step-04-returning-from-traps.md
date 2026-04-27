# Step 4: Returning from a Trap

The fourth milestone turns trap handling from "print and stop" into "handle and resume".

Previously, the kernel could enter a trap handler, print `scause`, `sepc`, and `stval`, and then spin forever. That proved that `stvec` and the trap entry worked, but it was not enough for a real kernel. Operating systems must often return from traps:

```text
system calls return to user code
timer interrupts return to the interrupted task
page faults may return after mapping memory
recoverable exceptions can continue after handling
```

In this step, the kernel handles a deliberate illegal instruction, advances `sepc`, restores CPU state, executes `sret`, and continues after the faulting instruction.

The expected flow is:

```text
kernel_main
  -> print hello
  -> execute illegal instruction
  -> CPU enters __trap_entry
  -> save registers into TrapFrame
  -> call Rust trap_handler
  -> decode TrapCause
  -> advance sepc by 4
  -> restore registers and CSRs
  -> sret
  -> continue after trap
```

## Moving Assembly into Rust

This step also removes the standalone `.S` files.

The old assembly entry files were replaced with Rust-side assembly:

```rust
core::arch::global_asm!(/* _start assembly */);
```

for the boot entry, and:

```rust
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.trap")]
pub unsafe extern "C" fn __trap_entry() {
    core::arch::naked_asm!(/* trap entry assembly */);
}
```

for the trap entry.

The trap entry is a naked function because the compiler must not emit a normal function prologue or epilogue. The trap entry owns the entire CPU context-saving contract. It decides how the stack is used, which registers are saved, and how control returns with `sret`.

Using `naked_asm!` also lets the assembly refer to Rust constants:

```rust
trap_frame_size = const TRAP_FRAME_SIZE,
sepc_offset = const TRAP_FRAME_SEPC_OFFSET,
handler = sym trap_handler,
```

That keeps the assembly closer to the Rust `TrapFrame` layout.

## Aligning the Trap Section

The linker script now places the trap section explicitly:

```ld
.text : {
    *(.text.entry)
    . = ALIGN(8);
    *(.text.trap .text.trap.*)
    *(.text .text.*)
}
```

This matters because `stvec` uses its low two bits as mode bits:

```text
stvec[XLEN-1:2] = trap vector base address
stvec[1:0]      = mode
```

The trap entry address must therefore be aligned so the low two bits are zero. Aligning `.text.trap` to 8 bytes is stricter than the minimum 4-byte alignment and keeps the trap vector safe.

## TrapFrame

The trap entry saves CPU state into a `TrapFrame`:

```rust
#[repr(C)]
pub struct TrapFrame {
    pub regs: [usize; 32],
    pub sepc: usize,
    pub sstatus: usize,
    pub scause: usize,
    pub stval: usize,
}
```

`#[repr(C)]` is important because assembly and Rust must agree on the exact field layout.

The frame contains:

```text
regs[0..32]  general-purpose register slots
sepc         exception program counter
sstatus      supervisor status register
scause       trap cause
stval        trap-specific value
```

The assembly saves the general-purpose registers, snapshots the trap CSRs, and passes the `TrapFrame` pointer to Rust in `a0`.

That follows the RISC-V calling convention:

```text
a0 = first function argument
```

So this assembly call:

```asm
mv a0, sp
call trap_handler
```

matches this Rust signature:

```rust
pub extern "C" fn trap_handler(tf: &mut TrapFrame)
```

## Decoding scause

Instead of directly printing raw `scause` names with a helper function, the kernel now parses trap causes into enums:

```rust
pub enum TrapCause {
    Exception(ExceptionCause),
    Interrupt(InterruptCause),
}
```

This keeps the raw hardware value in the `TrapFrame`, while giving the Rust handler a readable representation:

```rust
let cause = TrapCause::from_scause(tf.scause);
```

For the current test, the important case is:

```rust
TrapCause::Exception(ExceptionCause::IllegalInstruction)
```

That corresponds to:

```text
scause = 2
```

## Advancing sepc

The test trap is caused by this instruction in `kernel_main`:

```rust
core::arch::asm!(".word 0");
```

`.word 0` emits a 4-byte zero instruction word. That is an illegal RISC-V instruction, so the CPU traps with:

```text
Illegal instruction
```

When the trap occurs, `sepc` points at the faulting instruction. If we returned without changing `sepc`, the CPU would execute the same illegal instruction again and trap forever.

To recover, the handler skips over the bad instruction:

```rust
tf.sepc += 4;
```

This works because the test instruction is exactly 4 bytes long.

## Returning with sret

After `trap_handler` returns, the naked trap entry restores the saved state, writes the possibly modified `sepc` back to the CSR, releases the trap frame from the stack, and executes:

```asm
sret
```

`sret` means Supervisor Return. It is not a normal function return. A normal `ret` jumps to `ra`, while `sret` resumes from the address stored in `sepc`.

In this test:

```text
original sepc = address of .word 0
new sepc      = original sepc + 4
```

So `sret` resumes after the illegal instruction.

## Expected Output

The kernel should now continue after the trap:

```text
Hello, RISC-V!

trap!!
  scause = Illegal instruction (2)
  sepc   = 0x...
  stval  = 0x0
  action = skip illegal instruction
after trap
```

Seeing `after trap` proves that the handler returned successfully.

## What This Step Proves

This step proves that:

1. The trap entry can save enough CPU state to call Rust safely.
2. Rust can inspect and modify a trap frame.
3. `scause` can be decoded into readable trap enums.
4. The handler can recover from a known exception.
5. Updating `sepc` changes where `sret` resumes execution.
6. The kernel can continue after a trap.

This is the beginning of a real trap subsystem.

## Next Step

The next useful milestone is to handle a deliberate `ecall`.

That will reuse the same trap return machinery, but the cause will be:

```text
Environment call from S-mode
```

An `ecall` test is a natural bridge toward system calls, even before the kernel has user mode tasks.
