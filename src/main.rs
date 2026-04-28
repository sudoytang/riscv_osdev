#![no_std]
#![no_main]

mod uart;
mod trap;


core::arch::global_asm!(
r#"
.section .text.entry
.global _start

_start:
    la sp, boot_stack_top
    call kernel_main

spin:
    j spin
"#
);




#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    trap::init();
    init_kernel_trap_stack();
    krnl_println!("Hello, RISC-V!\n");

    unsafe {
        // trigger a trap of illegal instruction
        core::arch::asm!(".word 0");
    }
    krnl_println!("After illegal instruction trap.");

    krnl_println!("Entering user mode...");
    enter_user_mode(user_main as *const () as usize, user_stack_top());
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}


// Stack
const STACK_SIZE: usize = 4096 * 16;

#[repr(align(16))]
struct Stack([u8; STACK_SIZE]);

impl Stack {
    const fn new() -> Self {
        Self([0; STACK_SIZE])
    }
}

static mut USER_STACK: Stack = Stack::new();
static mut KERNEL_TRAP_STACK: Stack = Stack::new();

fn user_stack_top() -> usize {
    unsafe {
        (core::ptr::addr_of_mut!(USER_STACK.0) as *mut u8).add(STACK_SIZE) as usize
    }
}

fn kernel_trap_stack_top() -> usize {
    unsafe {
        (core::ptr::addr_of_mut!(KERNEL_TRAP_STACK.0) as *mut u8).add(STACK_SIZE) as usize
    }
}

// Convention: When running in S-mode, the sscratch is 0
// At the point before entering U-mode, set sscratch to kernel_trap_stack_top
fn init_kernel_trap_stack() {
    unsafe {
        core::arch::asm!(
            "csrw sscratch, zero",
        );
    }
}

struct UserStdout;

impl core::fmt::Write for UserStdout {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        user_print_str(s);
        Ok(())
    }
}

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

#[unsafe(no_mangle)]
extern "C" fn user_main() -> ! {
    // krnl_println!("After ecall: returned value = {}", ret);
    // ^^^ You shouldn't use this in user mode, it is kernel only and it won't work
    //     once you implemented Virtual Memory.
    // Use user_println! instead
    user_println!("Hello from U-mode via syscall!");

    loop {}
}

fn enter_user_mode(entry: usize, user_sp: usize) -> ! {
    unsafe {
        core::arch::asm!(
            "csrw sepc, {entry}",
            // Set sscratch to kernel trap stack top before entering U-mode.
            "csrw sscratch, {kernel_sp}",
            // Clear sstatus.SPP so sret returns to U-mode.
            "csrr t0, sstatus",
            "li t1, {sstatus_spp}",
            "not t1, t1",
            "and t0, t0, t1",
            // Set SPIE so interrupts are enabled after sret.
            "li t1, {sstatus_spie}",
            "or t0, t0, t1",
            "csrw sstatus, t0",
            // Switch to the user stack and enter user code.
            "mv sp, {user_sp}",
            "sret",
            entry = in(reg) entry,
            user_sp = in(reg) user_sp,
            kernel_sp = in(reg) kernel_trap_stack_top(),
            sstatus_spp = const 1 << 8,
            sstatus_spie = const 1 << 5,
            options(noreturn),
        )
    }
}
