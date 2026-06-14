# riscv_osdev

A learning project for building a small RISC-V operating system in Rust and running it with QEMU.

The current milestone (Step 14) boots a `no_std` Rust kernel on QEMU's RISC-V `virt` machine, enables Sv39 paging, handles traps and syscalls, and runs cooperative multitasking with repeatable `spawn` from an embedded program image store.

## Current Capabilities

- QEMU virt + OpenSBI boot
- UART console output
- Sv39 virtual memory with a free-list frame allocator
- Trap handling with full `TrapFrame` save/restore
- User-mode execution with `write`, `exit`, and `yield` syscalls
- Cooperative round-robin scheduling across multiple tasks
- Embedded `user.bin` as a read-only program image store
- `spawn` copies program images into per-task private pages so the same entry can be run again after exit

## Run

```bash
cargo run
```

This builds the kernel for `riscv64gc-unknown-none-elf`, compiles the user program, embeds it into the kernel, and launches QEMU.

To exit QEMU, press `Ctrl-a`, then `x`.

## Learning Path

Step-by-step notes live in `docs/`:

- `docs/step-01-rust-riscv-qemu-bootstrap.md` through `docs/step-13-cooperative-multitasking.md`
- `docs/step-14-spawn-and-program-image-store.md`

## Next Step

Timer interrupts and preemptive scheduling.
