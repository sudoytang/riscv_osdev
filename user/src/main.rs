#![no_std]
#![no_main]

mod syscall;

core::arch::global_asm!(
r#"
.section .text.entry
.global _start

_start:
    j user_main_0
    j user_main_1
"#
);

struct UserStdout;

impl core::fmt::Write for UserStdout {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        syscall::write(syscall::STDOUT_FD, s.as_bytes());
        Ok(())
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

#[unsafe(no_mangle)]
pub extern "C" fn user_main_0() -> ! {
    user_println!("Task 0");
    user_println!("Hello from U-mode via syscall!");
    syscall::yield_();

    user_println!("Let's trigger a trap of illegal instruction...");
    unsafe {
        core::arch::asm!(".word 0");
    }
    user_println!("Illegal instruction trap handled.");
    user_println!("Bye!");

    syscall::exit(0);
}

#[unsafe(no_mangle)]
pub extern "C" fn user_main_1() -> ! {
    user_println!("Task 1");
    user_println!("Hello from U-mode via syscall!");
    syscall::yield_();

    user_println!("Task 1 resumed.");
    user_println!("Bye!");
    syscall::exit(0);
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
