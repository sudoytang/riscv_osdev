#![no_std]
#![no_main]

mod uart;
mod trap;


core::arch::global_asm!(include_str!("entry.S"));

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    trap::init();
    println!("Hello, RISC-V!\n");

    unsafe {
        // trigger a trap
        core::arch::asm!(".word 0");
    }

    loop {}
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}