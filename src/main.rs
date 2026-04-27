#![no_std]
#![no_main]

mod uart;


core::arch::global_asm!(include_str!("entry.S"));

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main() -> ! {
    println!("Hello, RISC-V!\n");
    loop {}
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}