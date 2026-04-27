# Step 2: Printing from the Kernel with UART

The second milestone is making the kernel prove that Rust code is running by printing text from inside `kernel_main`.

After the first step, the boot path already worked:

```text
QEMU -> OpenSBI -> _start -> kernel_main
```

However, `kernel_main` only looped forever. That made the kernel valid, but silent. In this step, we add a tiny UART output path so the kernel can print:

```text
Hello, RISC-V!
```

## Why UART

Early kernels usually need a simple debug console before they have drivers, interrupts, memory allocation, filesystems, or graphics.

On QEMU's RISC-V `virt` machine, the early console device is a 16550-compatible UART. OpenSBI also reports it in the boot log:

```text
Platform Console Device     : uart8250
```

For this machine, UART0 is memory-mapped at:

```text
0x1000_0000
```

This means the kernel can interact with the UART by reading and writing specific memory addresses. These accesses are not normal RAM accesses. They are MMIO operations, where a load or store talks to a hardware device.

## UART Registers Used

The current implementation uses two pieces of the UART register layout:

```text
UART0_BASE + 0  transmit holding register
UART0_BASE + 5  line status register
```

The transmit holding register is where the kernel writes a byte to send it.

The line status register tells us whether the transmitter is ready. Bit 5 means the transmit holding register is empty:

```text
1 << 5
```

Before writing each byte, the kernel waits until that bit is set. This avoids writing a new byte before the UART is ready to accept it.

## Volatile MMIO Access

The UART code uses volatile reads and writes:

```rust
read_volatile(...)
write_volatile(...)
```

This matters because MMIO accesses have side effects outside the CPU and memory model. A normal memory access might be optimized away, reordered, or merged by the compiler if it appears unnecessary.

Volatile access tells the compiler:

> This read or write must really happen.

That is exactly what we need for device registers.

## The UART Output Path

The low-level output path is:

```text
println!
  -> uart::print
  -> core::fmt::Write::write_fmt
  -> Uart0::write_str
  -> uart0_puts
  -> uart0_putchar
  -> write_volatile(UART0_BASE, byte)
```

At the lowest level, one byte is sent by waiting for UART0 to become writable and then writing the byte to the UART base address.

The implementation also exposes formatting support through Rust's `core::fmt::Write` trait. This lets the kernel use `format_args!` and simple `print!` / `println!` macros without depending on `std`.

That is important because this project is still a bare-metal `no_std` kernel.

## Kernel Entry

The kernel entry now calls:

```rust
println!("Hello, RISC-V!\n");
```

When QEMU runs the kernel, the output appears after the OpenSBI boot log. That confirms:

1. OpenSBI entered our kernel.
2. `_start` set up a stack.
3. Control reached Rust code.
4. The kernel can write to a memory-mapped device.
5. The terminal output path is working.

## What This Step Proves

This step proves that the kernel is no longer just bootable. It can communicate with the outside world.

That gives us a basic debugging tool for the next stages:

```text
trap handling
timer interrupts
memory management
page table debugging
system call tracing
```

For early OS development, this kind of UART output is one of the most useful pieces of infrastructure.

## Next Step

The next milestone is trap handling.

We want to configure the RISC-V `stvec` register so exceptions and interrupts enter our own trap entry. Then we can deliberately trigger an exception and print:

```text
trap!
scause = ...
sepc   = ...
stval  = ...
```

That will be the foundation for exceptions, timer interrupts, page faults, and eventually system calls.
