#![no_std]
#![no_main]

mod mm;
mod program;
mod syscall;
mod task;
mod trap;
mod uart;

// Forces a kernel rebuild when the embedded user binary changes (see build.rs).
const _: &str = env!("USER_BIN_CHECKSUM");

// Integrate user binary code into kernel memory space
core::arch::global_asm!(concat!(
r#"
.section .user_bin, "a"
.align 12
.global user_bin_start
user_bin_start:
"#,
".incbin \"", env!("USER_BIN_PATH"), "\"\n",
r#"
.align 12
.global user_bin_end
user_bin_end:
"#
));

unsafe extern "C" {
    static ekernel: [u8; 0];
}

// QEMU virt machine RAM: 128MB
// TODO: Read DTB for real memory amount later
pub const PHYSICAL_MEMORY_END: usize = 0x80000000 + 128 * 1024 * 1024;


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
    let ekernel_addr = core::ptr::addr_of!(ekernel) as usize;
    mm::init_frame_allocator(ekernel_addr, PHYSICAL_MEMORY_END);
    mm::map_kernel_gigapages();
    mm::enable_paging();

    krnl_println!("[kernel_main] Hello, RISC-V!\n");


    krnl_println!("[kernel_main] ekernel @ {:#x}", ekernel_addr);

    krnl_println!("[kernel_main] Let's trigger a trap of illegal instruction...");
    unsafe {
        // trigger a trap of illegal instruction
        core::arch::asm!(".word 0");
    }
    krnl_println!("[kernel_main] Illegal instruction trap handled.");

    krnl_println!("[kernel_main] Spawning initial tasks...");
    task::spawn(&program::USER_MAIN_0);
    task::spawn(&program::USER_MAIN_1);
    task::spawn(&program::USER_MAIN_1);
    task::schedule_until_idle();

    krnl_println!("[kernel_main] Re-spawning user_main_0 to validate repeat run...");
    task::spawn(&program::USER_MAIN_0);
    task::schedule_until_idle();

    krnl_println!("[kernel_main] All validations complete, kernel halted.");
    loop {}
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    krnl_println!("[panic_handler] {}", info);
    loop {}
}

// Convention: When running in S-mode, sscratch = 0.
// Each task sets sscratch to its own kernel stack top before entering U-mode.
fn init_kernel_trap_stack() {
    unsafe {
        core::arch::asm!("csrw sscratch, zero");
    }
}

// kernel_sp: the task's kernel stack top, written to sscratch so that
// U-mode traps land on the correct per-task kernel stack.
pub fn enter_user_mode(entry: usize, user_sp: usize, kernel_sp: usize) -> ! {
    unsafe {
        core::arch::asm!(
            "csrw sepc, {entry}",
            // Set sscratch to this task's kernel stack top before entering U-mode.
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
            kernel_sp = in(reg) kernel_sp,
            sstatus_spp = const 1 << 8,
            sstatus_spie = const 1 << 5,
            options(noreturn),
        )
    }
}
