# Step 1: Bootstrapping a Rust RISC-V Kernel in QEMU

This project starts with the smallest useful milestone for learning operating system development:

> Build a Rust bare-metal kernel, load it with QEMU's RISC-V `virt` machine, and reach our own kernel entry point.

At this point, the kernel does not print anything yet. That is expected. The important result is that the full boot path works:

```text
Cargo
  -> rustc for riscv64gc-unknown-none-elf
  -> linker script places the kernel at 0x80200000
  -> QEMU starts the RISC-V virt machine
  -> OpenSBI runs first
  -> OpenSBI jumps to our kernel in S-mode
  -> our assembly entry sets up the stack
  -> Rust kernel_main runs
```

This is the foundation we need before adding UART output, traps, paging, memory management, processes, or system calls.

## Why We Use a Bare-Metal Rust Target

The project is configured for:

```text
riscv64gc-unknown-none-elf
```

This target means:

```text
riscv64gc  64-bit RISC-V with common general-purpose extensions
unknown    no specific hardware vendor
none       no operating system
elf        output an ELF executable
```

The `none` part is especially important. We are not building a macOS or Linux program. We are building a program that runs without an operating system underneath it.

That is why the Rust code uses:

```rust
#![no_std]
#![no_main]
```

`#![no_std]` means the kernel cannot use Rust's standard library. There is no file system, allocator, thread runtime, stdout, or OS service available yet. Only `core` is available.

`#![no_main]` means Rust should not generate the normal hosted-program entry path. A normal Rust binary expects an operating system to call `main`. Our kernel is entered directly by firmware, so we provide our own low-level entry path.

## Cargo Configuration

The project uses `.cargo/config.toml` to avoid typing the target and QEMU command every time:

```toml
[build]
target = "riscv64gc-unknown-none-elf"

[target.riscv64gc-unknown-none-elf]
runner = "qemu-system-riscv64 -machine virt -nographic -bios default -kernel"

rustflags = [
    # Pass the linker script to the linker.
    "-C", "link-arg=-Tlinker.ld",
]
```

The `[build]` section tells Cargo to compile for RISC-V bare metal by default.

The `runner` line tells Cargo how to run the compiled kernel. When we execute:

```bash
cargo run
```

Cargo builds the kernel and then runs something equivalent to:

```bash
qemu-system-riscv64 \
  -machine virt \
  -nographic \
  -bios default \
  -kernel target/riscv64gc-unknown-none-elf/debug/riscv_osdev
```

Cargo automatically appends the compiled binary path to the runner command.

The `rustflags` section passes our linker script to the linker:

```text
-Tlinker.ld
```

For a normal application, the operating system and default linker layout decide where code and data go. For a kernel, we must control the memory layout ourselves.

## The Linker Script

The linker script is `linker.ld`:

```ld
OUTPUT_ARCH(riscv)
ENTRY(_start)

BASE_ADDRESS = 0x80200000;
```

`OUTPUT_ARCH(riscv)` tells the linker that the output is for RISC-V.

`ENTRY(_start)` tells the linker that `_start` is the entry symbol.

`BASE_ADDRESS = 0x80200000;` is the address where this kernel is linked to run. With QEMU's RISC-V `virt` machine and OpenSBI, the kernel is commonly entered at:

```text
0x80200000
```

The script then places sections in memory:

```ld
.text : {
    *(.text.entry)
    *(.text .text.*)
}
```

`.text` contains executable code. We place `.text.entry` first so the low-level entry code is located at the beginning of the kernel image.

```ld
.rodata : {
    *(.rodata .rodata.*)
}
```

`.rodata` contains read-only data, such as string literals.

```ld
.data : {
    *(.data .data.*)
}
```

`.data` contains initialized global data.

```ld
.bss : {
    *(.bss .bss.*)
    *(COMMON)
}
```

`.bss` contains zero-initialized global data.

At the end of the linker script, we reserve space for the initial boot stack:

```ld
. = ALIGN(16);
. += 4096 * 16;
boot_stack_top = .;
```

This reserves 64 KiB for the stack and defines a symbol called `boot_stack_top`. The assembly entry code uses this symbol to initialize the RISC-V stack pointer.

## The Assembly Entry Point

The file `src/entry.S` contains the earliest code that our kernel controls:

```asm
.section .text.entry
.global _start

_start:
    la sp, boot_stack_top
    call kernel_main

spin:
    j spin
```

The first line places this code into the `.text.entry` section:

```asm
.section .text.entry
```

Because the linker script puts `.text.entry` before normal `.text`, this code becomes the first part of our kernel's code section.

This line exports `_start`:

```asm
.global _start
```

The linker script uses `_start` as the kernel entry symbol:

```ld
ENTRY(_start)
```

The `_start` label is where execution begins after OpenSBI jumps into our kernel:

```asm
_start:
```

Before entering Rust code, we must set up a valid stack:

```asm
la sp, boot_stack_top
```

`sp` is the RISC-V stack pointer register. `la` loads the address of `boot_stack_top`, which was defined by `linker.ld`.

This is why a small assembly entry is useful. A normal Rust function may use the stack immediately, so we set `sp` before calling Rust.

Then we call the Rust kernel entry:

```asm
call kernel_main
```

If `kernel_main` ever returns, the CPU enters an infinite loop:

```asm
spin:
    j spin
```

For a kernel, returning from the main entry point does not make sense. There is no operating system waiting for this program to exit.

## The Rust Kernel Entry

The Rust side currently looks like this:

```rust
#![no_std]
#![no_main]

use core::arch::global_asm;

global_asm!(include_str!("entry.S"));

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    loop {}
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
```

This line includes the assembly entry file in the Rust crate:

```rust
global_asm!(include_str!("entry.S"));
```

Without it, Cargo would compile `src/main.rs`, but it would not automatically compile `src/entry.S`.

The Rust function called by assembly is:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    loop {}
}
```

`#[unsafe(no_mangle)]` keeps the symbol name as `kernel_main`. Without this, Rust would mangle the function name, and the assembly instruction `call kernel_main` would not find the expected symbol.

`extern "C"` gives the function a C-compatible ABI, which makes it straightforward to call from assembly.

The return type `!` means this function never returns. That matches the behavior of a kernel entry point.

The panic handler is required in a `no_std` Rust binary:

```rust
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
```

In a normal Rust program, the standard library provides panic behavior. In a bare-metal kernel, we must provide it ourselves.

## What Happens When We Run It

Running:

```bash
cargo run
```

builds the kernel and starts QEMU. The terminal shows the OpenSBI banner:

```text
OpenSBI v1.7
Platform Name               : riscv-virtio,qemu
Platform Console Device     : uart8250
Domain0 Next Address        : 0x0000000080200000
Domain0 Next Mode           : S-mode
```

This is correct.

OpenSBI is firmware for RISC-V systems. In our current setup, the boot flow is:

```text
QEMU -> OpenSBI -> our kernel
```

OpenSBI runs in Machine mode, also called M-mode. Our kernel runs in Supervisor mode, also called S-mode.

The most important lines are:

```text
Domain0 Next Address        : 0x0000000080200000
Domain0 Next Mode           : S-mode
```

`Next Address` matches the base address in our linker script:

```ld
BASE_ADDRESS = 0x80200000;
```

`Next Mode` tells us OpenSBI is going to enter our kernel in S-mode, which is the normal privilege level for an operating system kernel on RISC-V.

After that, our kernel enters `kernel_main`, which currently only loops forever. Because we have not written UART output yet, there is no kernel message printed after the OpenSBI banner.

That is still a successful first milestone.

## What This Step Proves

This step proves that:

1. Rust can compile the project for a RISC-V bare-metal target.
2. The linker script places the kernel at the expected address.
3. QEMU can start the RISC-V `virt` machine.
4. OpenSBI can load and enter our kernel.
5. Our assembly `_start` can set up the stack pointer.
6. Control can pass from assembly into Rust.
7. The Rust kernel can run without `std` or a normal `main`.

This is not a complete operating system yet, but it is the first real bootstrapping layer.

## Next Step

The next milestone is to prove that our Rust code is actively running by writing to the QEMU `virt` machine's UART device.

The expected result will be:

```text
Hello, RISC-V!
```

That will introduce memory-mapped I/O, specifically writing bytes to the UART device exposed by QEMU.
