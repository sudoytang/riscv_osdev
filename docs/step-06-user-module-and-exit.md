# Step 6: Splitting User Code and Adding SYS_EXIT

The sixth milestone cleans up the user-mode code path and adds the first terminating syscall.

The previous step proved that U-mode code can call into the S-mode kernel through `ecall`, and that user output can go through `SYS_PUTCHAR`. After that, `main.rs` had started to mix several responsibilities:

```text
kernel boot flow
trap stack setup
entering U-mode
user-mode syscall wrappers
user-mode printing macros
user_main
```

This step separates the user-side code into `src/user.rs` and adds:

```text
SYS_EXIT = 2
```

Now user code can explicitly tell the kernel that it is done.

## User Code Module

The project now has:

```text
src/main.rs   kernel initialization and entering U-mode
src/user.rs   user-mode demo code and user-side syscall wrappers
src/trap.rs   trap entry, trap handling, and kernel syscall dispatch
```

`main.rs` imports the user entry point:

```rust
mod user;

use user::user_main;
```

Then the kernel enters U-mode with:

```rust
enter_user_mode(user_main as *const () as usize, user_stack_top());
```

The user stack still belongs to the kernel side for now:

```rust
static mut USER_STACK: Stack = Stack::new();
```

That is intentional. The user stack is a kernel-managed resource. Later, when the kernel has a task abstraction, user stacks should move into a task or process module rather than into the user program itself.

## User-Side Syscall Wrappers

`src/user.rs` contains user-side wrappers around `ecall`.

For `SYS_PUTCHAR`, the wrapper is:

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

The user-side convention is:

```text
a7 = syscall number
a0 = first argument
```

This mirrors the RISC-V syscall calling style used by larger systems, while keeping our syscall table small and project-specific.

## User Printing Macros

The user module now owns:

```rust
user_print!(...)
user_println!(...)
```

These macros use a small `core::fmt::Write` implementation:

```rust
struct UserStdout;
```

Formatting happens in user code, but actual output goes through syscalls one byte at a time:

```text
user_println!
  -> UserStdout::write_str
  -> user_print_str
  -> user_putchar
  -> ecall SYS_PUTCHAR
  -> S-mode trap handler
  -> kernel UART output
```

This is not yet efficient, because each byte causes a trap. It is still useful at this stage because it proves the user/kernel boundary is working.

Later, this should be replaced with a `SYS_WRITE(buf, len)` syscall so a whole buffer can be written with one trap.

## SYS_EXIT

`SYS_EXIT` is the second real syscall:

```rust
pub enum Syscall {
    Reserved,
    PutChar,
    Exit,
    Unknown(usize),
}
```

The syscall numbers are:

```text
0 = reserved
1 = SYS_PUTCHAR
2 = SYS_EXIT
```

User code calls exit with:

```rust
fn user_exit(exit_code: usize) -> ! {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") trap::Syscall::SYS_EXIT,
            in("a0") exit_code as usize,
            options(noreturn),
        );
    }
}
```

`options(noreturn)` is valid for the current design because `SYS_EXIT` does not return to the user program.

## Kernel-Side Exit Handling

The kernel handles `SYS_EXIT` in `Syscall::krnl_handle`:

```rust
Self::Exit => {
    let exit_code = tf.regs[A0];
    krnl_println!("  User exited with code {}", exit_code);
    krnl_println!("  Kernel halted");
    loop {}
}
```

Right now there is only one user program and no scheduler. That means the smallest useful behavior is:

```text
print the exit code
halt the kernel
```

This is not the final form of `exit`. In a future kernel with tasks, `SYS_EXIT` should:

```text
mark the current task as exited
store the exit code
release task resources
wake any waiter
schedule another runnable task
```

But the syscall semantic is already correct:

> User code does not end itself. It asks the kernel to terminate it.

## Current User Program

The current user program demonstrates:

```rust
pub extern "C" fn user_main() -> ! {
    user_println!("Hello from U-mode via syscall!");

    user_println!("Let's trigger a trap of illegal instruction...");
    unsafe {
        core::arch::asm!(".word 0");
    }
    user_println!("Illegal instruction trap handled.");
    user_println!("Bye!");

    user_exit(0);
}
```

This proves several paths:

```text
U-mode SYS_PUTCHAR
U-mode illegal instruction trap
returning from a handled U-mode exception
U-mode SYS_EXIT
```

## Expected Output

The output should look like:

```text
Hello, RISC-V!

Let's trigger a trap of illegal instruction...
trap!!
  scause = Illegal instruction (2)
  ...
Illegal instruction trap handled.
Entering user mode...
Hello from U-mode via syscall!
Let's trigger a trap of illegal instruction...
trap!!
  scause = Illegal instruction (2)
  ...
Illegal instruction trap handled.
Bye!
  User exited with code 0
  Kernel halted
```

The exact `sepc` values can change as code layout changes.

## What This Step Proves

This step proves that:

1. User-mode demo code can live outside `main.rs`.
2. User code can use syscall wrappers instead of touching kernel internals.
3. The kernel can dispatch multiple syscall numbers.
4. `SYS_EXIT` gives user code an explicit termination path.
5. U-mode exceptions and U-mode syscalls both return through the same trap machinery.

The next natural step is to replace byte-at-a-time `SYS_PUTCHAR` output with a buffer-based `SYS_WRITE`.
