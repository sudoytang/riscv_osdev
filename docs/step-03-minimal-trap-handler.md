# Step 3: Building a Minimal RISC-V Trap Handler

The third milestone is teaching the kernel how to receive a CPU trap.

So far, the kernel can boot and print through UART. That proves the basic path works:

```text
QEMU -> OpenSBI -> _start -> kernel_main -> UART output
```

The next important operating system concept is trap handling. A trap is the mechanism RISC-V uses to transfer control to the kernel when something special happens, such as:

```text
illegal instruction
page fault
system call
timer interrupt
external interrupt
```

In this step, we implement the smallest useful version:

```text
configure stvec
  -> deliberately execute an illegal instruction
  -> enter our assembly trap entry
  -> call a Rust trap handler
  -> print scause, sepc, and stval
  -> stop
```

This is not a complete trap system yet. It does not save and restore the full CPU context, and it does not return from the trap. The goal is only to prove that exceptions are routed into our own kernel code.

## The Trap Entry

The assembly trap entry lives in `src/trap.S`:

```asm
.section .text.trap
.balign 8
.global __trap_entry

__trap_entry:
    call trap_handler

1:
    j 1b
```

The symbol `__trap_entry` is the address we write into the RISC-V `stvec` CSR.

When a trap happens in S-mode, the CPU jumps to the address stored in `stvec`. That makes `stvec` the root of S-mode exception and interrupt handling.

The trap entry currently calls the Rust function `trap_handler`. If that function ever returned, the assembly code would spin forever.

## Why Alignment Matters

The `.balign 8` line is important:

```asm
.balign 8
```

RISC-V's `stvec` register does not store only an address. Its low two bits are mode bits:

```text
stvec[XLEN-1:2] = trap vector base address
stvec[1:0]      = mode
```

The common modes are:

```text
0 = Direct
1 = Vectored
```

This means the trap entry address must have its low two bits cleared. In practice, the address must be at least 4-byte aligned.

We saw a real example of what can go wrong:

```text
0000000080200012 T __trap_entry
```

The address ended in `0x...12`, so the low bits were not suitable for `stvec`. Writing that value directly into `stvec` caused confusing behavior.

Aligning the symbol fixes this by ensuring the entry address is safe to use as an `stvec` base. `.balign 4` would be enough, but `.balign 8` is also valid because 8-byte alignment is stricter than 4-byte alignment.

## Configuring stvec

The Rust side includes the assembly file and declares the external symbol:

```rust
core::arch::global_asm!(include_str!("trap.S"));

unsafe extern "C" {
    fn __trap_entry();
}
```

Then `trap::init()` writes the trap entry address into `stvec`:

```rust
pub fn init() {
    unsafe {
        core::arch::asm!("csrw stvec, {}", in(reg) __trap_entry as *const () as usize);
    }
}
```

After this write, S-mode traps should enter `__trap_entry`.

## Triggering a Trap

In `kernel_main`, we initialize trap handling and then deliberately execute an invalid instruction:

```rust
trap::init();
println!("Hello, RISC-V!\n");

unsafe {
    // trigger a trap
    core::arch::asm!(".word 0");
}
```

The directive:

```asm
.word 0
```

emits four zero bytes as an instruction word. On RISC-V, this is not a valid instruction, so the CPU raises an illegal instruction exception.

This is a controlled way to test that our trap path works.

## Reading Trap CSRs

The Rust trap handler reads three important CSRs:

```rust
core::arch::asm!("csrr {}, scause", out(reg) scause);
core::arch::asm!("csrr {}, sepc", out(reg) sepc);
core::arch::asm!("csrr {}, stval", out(reg) stval);
```

These registers explain what happened.

`scause` stores the trap cause. It tells us whether the trap was an exception or interrupt, and gives the cause code.

`sepc` stores the program counter where the exception happened. For our test, this points at the `.word 0` instruction.

`stval` stores extra trap-specific information. For an illegal instruction exception, it may contain the illegal instruction bits, or it may be zero depending on the implementation and instruction.

## Decoding scause

The handler now maps common `scause` values to readable names.

For example:

```text
2 = Illegal instruction
```

So instead of printing only:

```text
scause = 2
```

the kernel can print something more useful:

```text
scause = Illegal instruction (2)
```

This small decoding layer will become more useful as the kernel starts handling page faults, environment calls, and timer interrupts.

## Expected Output

With the illegal instruction test enabled, the expected output is similar to:

```text
Hello, RISC-V!

trap!!
  scause = Illegal instruction (2)
  sepc   = 0x80200042
  stval  = 0x0
```

The exact `sepc` value can change as code layout changes.

The important part is that the kernel enters the trap handler and prints a decoded cause.

## What This Step Proves

This step proves that:

1. The kernel can configure the S-mode trap vector.
2. The CPU can transfer control into our trap entry.
3. Assembly can bridge trap entry into Rust code.
4. The Rust handler can inspect trap CSRs.
5. The kernel can decode and print useful trap information.
6. Trap vector alignment matters because `stvec` uses low bits as mode bits.

This is the foundation for real kernel exception and interrupt handling.

## Next Step

The next milestone is to make some traps return instead of stopping forever.

For example, the kernel can handle the illegal instruction test by:

```text
read sepc
advance sepc past the bad instruction
restore registers
execute sret
continue after the trap
```

To do that correctly, the trap entry must save CPU registers into a trap frame, call Rust with that context, restore the registers, and return with `sret`.

That will turn the current "print and stop" trap handler into the beginning of a real trap subsystem.
