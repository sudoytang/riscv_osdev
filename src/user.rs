use crate::trap;

struct UserStdout;



impl core::fmt::Write for UserStdout {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        user_print_str(s);
        Ok(())
    }
}

// Wrapper for syscall 1: SYS_PUTCHAR
fn user_putchar(ch: u8) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") trap::Syscall::SYS_PUTCHAR,
            in("a0") ch as usize,
        );
    }
}

fn user_print_str(s: &str) {
    for ch in s.bytes() {
        user_putchar(ch);
    }
}

fn user_print_fmt(args: core::fmt::Arguments) {
    use core::fmt::Write;

    UserStdout.write_fmt(args).unwrap();
}

macro_rules! user_print {
    ($($arg:tt)*) => {
        user_print_fmt(format_args!($($arg)*));
    };
}

macro_rules! user_println {
    () => {
        user_print!("\n");
    };
    ($($arg:tt)*) => {
        user_print!("{}\n", format_args!($($arg)*));
    };
}

// Wrapper for syscall 2: SYS_EXIT
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

#[unsafe(no_mangle)]
pub extern "C" fn user_main() -> ! {
    // krnl_println!("After ecall: returned value = {}", ret);
    // ^^^ You shouldn't use this in user mode, it is kernel only and it won't work
    //     once you implemented Virtual Memory.
    // Use user_println! instead
    user_println!("Hello from U-mode via syscall!");

    user_println!("Let's trigger a trap of illegal instruction...");
    unsafe {
        // trigger a trap of illegal instruction in user mode
        core::arch::asm!(".word 0");
    }
    user_println!("Illegal instruction trap handled.");
    user_println!("Bye!");


    user_exit(0);
}
