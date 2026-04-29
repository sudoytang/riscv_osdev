#![no_std]
#![no_main]

mod mm;
mod syscall;
mod task;
mod trap;
mod uart;
mod user;

use user::{user_main_0, user_main_1};


unsafe extern "C" {
    static ekernel: [u8; 0];
}



// Entry point
// This sets up the boot stack and calls kernel_main
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
    mm::map_kernel_gigapages();
    mm::enable_paging();

    krnl_println!("Hello, RISC-V!\n");


    let ekernel_addr = core::ptr::addr_of!(ekernel) as usize;
    krnl_println!("ekernel @ {:#x}", ekernel_addr);

    krnl_println!("Let's trigger a trap of illegal instruction...");
    unsafe {
        // trigger a trap of illegal instruction
        core::arch::asm!(".word 0");
    }
    krnl_println!("Illegal instruction trap handled.");

    task::init_task(0, user_main_0 as *const () as usize, task::user_stack_top(0));
    task::init_task(1, user_main_1 as *const () as usize, task::user_stack_top(1));

    // Now this page is only for S-mode, we cannot run user task
    // Temporarily disable this
    // task::schedule();
    loop {}
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


static mut KERNEL_TRAP_STACK: Stack = Stack::new();

fn kernel_trap_stack_top() -> usize {
    unsafe { (core::ptr::addr_of_mut!(KERNEL_TRAP_STACK.0) as *mut u8).add(STACK_SIZE) as usize }
}

// Convention: When running in S-mode, the sscratch is 0
// At the point before entering U-mode, set sscratch to kernel_trap_stack_top
fn init_kernel_trap_stack() {
    unsafe {
        core::arch::asm!("csrw sscratch, zero",);
    }
}



pub fn enter_user_mode(entry: usize, user_sp: usize) -> ! {
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
