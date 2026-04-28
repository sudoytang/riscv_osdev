# Step 7: SYS_WRITE and a Shared Syscall ABI

The seventh milestone replaces byte-at-a-time output with a buffer-based write syscall and separates the syscall ABI from the trap handler.

Before this step, user printing used:

```text
SYS_PUTCHAR(byte)
```

That worked, but it meant every output byte caused a separate `ecall`. A string with 30 bytes caused 30 traps. This was useful for proving the syscall path, but it is not the shape of a real OS interface.

This step changes user output to:

```text
SYS_WRITE(fd, buf, len)
```

Now user printing can send a whole buffer with one syscall.

## Shared Syscall ABI

The syscall ABI now lives in `src/syscall.rs`:

```rust
pub const STDOUT_FD: usize = 1;

pub enum Syscall {
    Reserved,
    Write,
    Exit,
    Unknown(usize),
}
```

The syscall numbers are:

```text
0 = reserved
1 = SYS_WRITE
2 = SYS_EXIT
```

This module is intentionally small. It describes the ABI shared by both sides:

```text
user.rs   uses syscall numbers to issue ecall
trap.rs   decodes syscall numbers after trap entry
```

The trap handler no longer owns the syscall enum. It only owns kernel-side dispatch:

```rust
fn handle_syscall(syscall: Syscall, tf: &mut TrapFrame)
```

That keeps the layers clearer:

```text
syscall.rs  what the syscall ABI is
user.rs     how user code invokes it
trap.rs     how the kernel handles it
```

## SYS_WRITE ABI

`SYS_WRITE` uses a Unix-like shape:

```text
a7 = SYS_WRITE
a0 = fd
a1 = buffer pointer
a2 = buffer length
return a0 = bytes written
```

For now, only stdout is supported:

```text
fd = 1
```

Any other fd returns `usize::MAX` as a simple error value.

## User-Side Wrapper

The user-side wrapper accepts a byte slice:

```rust
fn user_write(fd: usize, buf: &[u8]) -> usize {
    let ret: usize;

    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") syscall::Syscall::SYS_WRITE,
            inlateout("a0") fd => ret,
            in("a1") buf.as_ptr() as usize,
            in("a2") buf.len(),
        );
    }

    ret
}
```

This is the right split:

```text
user API:    &[u8]
syscall ABI: raw pointer + length in registers
kernel side: reads pointer + length from TrapFrame
```

Text output is layered on top:

```rust
fn user_write_str(fd: usize, s: &str) {
    user_write(fd, s.as_bytes());
}
```

The formatting macros still use `core::fmt::Write`, but `write_str` now performs one `SYS_WRITE` per formatted string slice rather than one syscall per byte.

## Kernel-Side Handling

The kernel handles `SYS_WRITE` in `trap.rs`:

```rust
Syscall::Write => {
    let fd = tf.regs[A0];
    if fd != STDOUT_FD {
        tf.regs[A0] = usize::MAX;
        return;
    }

    let ptr = tf.regs[A1] as *const u8;
    let size = tf.regs[A2];
    let bytes: &[u8] = unsafe { core::slice::from_raw_parts(ptr, size) };

    let mut count = 0;
    for &b in bytes {
        krnl_print!("{}", b as char);
        count += 1;
    }

    tf.regs[A0] = count;
}
```

This is still a teaching implementation. It trusts the user pointer and length.

Later, once virtual memory exists, this path must become something like:

```text
copy_from_user(buf, len)
validate user address range
handle buffers crossing page boundaries
reject kernel addresses from user mode
```

For now, it is enough to demonstrate the ABI and the syscall data path.

## Why SYS_PUTCHAR Was Removed

`SYS_PUTCHAR` was useful as the first syscall because it avoided pointers. But once `SYS_WRITE` exists, single-character output can be expressed as:

```text
write(fd, ptr, 1)
```

So `putchar` should become a user-side helper if needed, not a kernel syscall.

The syscall table is now cleaner:

```text
SYS_WRITE
SYS_EXIT
```

## What This Step Proves

This step proves that:

1. User code can pass a pointer and length to the kernel.
2. The kernel can read a user-provided buffer.
3. User printing can use a buffer syscall instead of one syscall per byte.
4. The syscall ABI can be shared without making `trap.rs` own all syscall definitions.
5. `trap.rs` can focus on trap handling and kernel-side dispatch.

## Next Step

The next useful improvement is to move more trap-related structure out of `trap.rs`.

`trap.rs` currently contains:

```text
trap entry assembly
TrapFrame
register index constants
trap cause decoding
syscall dispatch
```

The next likely cleanup is to extract one of these:

```text
register.rs   register index constants
trap_frame.rs TrapFrame layout
cause.rs      TrapCause decoding
```

After that, the project will be ready for the next major OS feature: a minimal task structure or virtual memory.
