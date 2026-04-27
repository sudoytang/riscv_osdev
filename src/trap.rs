use crate::println;

core::arch::global_asm!(include_str!("trap.S"));

unsafe extern "C" {
    fn __trap_entry();
}

pub fn init() {
    unsafe {
        core::arch::asm!("csrw stvec, {}", in(reg) __trap_entry as *const () as usize);
    }
}

fn scause_name(scause: usize) -> &'static str {
    let is_interrupt = (scause >> (usize::BITS - 1)) != 0;
    let code = scause & !(1usize << (usize::BITS - 1));
    if is_interrupt {
        match code {
            5 => "Supervisor timer interrupt",
            9 => "Supervisor external interrupt",
            _ => "Unknown interrupt",
        }
    } else {
        match code {
            0 => "Instruction address misaligned",
            1 => "Instruction access fault",
            2 => "Illegal instruction",
            5 => "Load access fault",
            7 => "Store/AMO access fault",
            8 => "Environment call from U-mode",
            9 => "Environment call from S-mode",
            12 => "Instruction page fault",
            13 => "Load page fault",
            15 => "Store/AMO page fault",
            _ => "Unknown exception",
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn trap_handler() -> ! {
    let scause: usize;
    let sepc: usize;
    let stval: usize;

    unsafe {
        core::arch::asm!("csrr {}, scause", out(reg) scause);
        core::arch::asm!("csrr {}, sepc", out(reg) sepc);
        core::arch::asm!("csrr {}, stval", out(reg) stval);
    }

    println!("trap!!");
    println!("  scause = {} ({})", scause_name(scause), scause);
    println!("  sepc   = {:#x}", sepc);
    println!("  stval  = {:#x}", stval);

    loop {}
}