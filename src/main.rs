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
    println!("Hello, RISC-V!\n");

    unsafe {
        // trigger a trap
        core::arch::asm!(".word 0");
    }

    println!("after trap");
    loop {}
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
