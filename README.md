# riscv_osdev

A learning project for building a small RISC-V operating system in Rust and running it with QEMU.

The current milestone boots a minimal `no_std` Rust kernel through OpenSBI on QEMU's RISC-V `virt` machine. The kernel sets up an assembly entry point, initializes the stack, enters Rust code, and then loops forever.

## Run

```bash
cargo run
```

This builds the kernel for `riscv64gc-unknown-none-elf` and launches it with QEMU.

To exit QEMU, press `Ctrl-a`, then `x`.
