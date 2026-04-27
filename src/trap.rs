use crate::println;

#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.trap")]
pub unsafe extern "C" fn __trap_entry() {
    core::arch::naked_asm!(
        // allocate space for TrapFrame
        "addi sp, sp, -{trap_frame_size}",
        // save registers
        // x0 is 0 so skip it

        "sd x1, 8(sp)",
        // x2 is sp, save it later
        "sd x3, 24(sp)",
        "sd x4, 32(sp)",
        "sd x5, 40(sp)",
        "sd x6, 48(sp)",
        "sd x7, 56(sp)",
        "sd x8, 64(sp)",
        "sd x9, 72(sp)",
        "sd x10, 80(sp)",
        "sd x11, 88(sp)",
        "sd x12, 96(sp)",
        "sd x13, 104(sp)",
        "sd x14, 112(sp)",
        "sd x15, 120(sp)",
        "sd x16, 128(sp)",
        "sd x17, 136(sp)",
        "sd x18, 144(sp)",
        "sd x19, 152(sp)",
        "sd x20, 160(sp)",
        "sd x21, 168(sp)",
        "sd x22, 176(sp)",
        "sd x23, 184(sp)",
        "sd x24, 192(sp)",
        "sd x25, 200(sp)",
        "sd x26, 208(sp)",
        "sd x27, 216(sp)",
        "sd x28, 224(sp)",
        "sd x29, 232(sp)",
        "sd x30, 240(sp)",
        "sd x31, 248(sp)",

        // save sp
        "addi a0, sp, 272",
        "sd a0, 16(sp)",
        // save sepc
        "csrr a0, sepc",
        "sd a0, {sepc_offset}(sp)",
        // save sstatus
        "csrr a0, sstatus",
        "sd a0, {sstatus_offset}(sp)",
        // save scause
        "csrr a0, scause",
        "sd a0, {scause_offset}(sp)",
        // save stval
        "csrr a0, stval",
        "sd a0, {stval_offset}(sp)",

        // call handler
        "mv a0, sp",
        "call {handler}",

        // restore sepc
        "ld a0, {sepc_offset}(sp)",
        "csrw sepc, a0",
        // restore sstatus
        "ld a0, {sstatus_offset}(sp)",
        "csrw sstatus, a0",
        // restore scause
        "ld a0, {scause_offset}(sp)",
        "csrw scause, a0",
        // restore stval
        "ld a0, {stval_offset}(sp)",
        "csrw stval, a0",

        // restore registers
        "ld x1, 8(sp)",
        // x2 is sp, use addi to restore
        "ld x3, 24(sp)",
        "ld x4, 32(sp)",
        "ld x5, 40(sp)",
        "ld x6, 48(sp)",
        "ld x7, 56(sp)",
        "ld x8, 64(sp)",
        "ld x9, 72(sp)",
        "ld x10, 80(sp)",
        "ld x11, 88(sp)",
        "ld x12, 96(sp)",
        "ld x13, 104(sp)",
        "ld x14, 112(sp)",
        "ld x15, 120(sp)",
        "ld x16, 128(sp)",
        "ld x17, 136(sp)",
        "ld x18, 144(sp)",
        "ld x19, 152(sp)",
        "ld x20, 160(sp)",
        "ld x21, 168(sp)",
        "ld x22, 176(sp)",
        "ld x23, 184(sp)",
        "ld x24, 192(sp)",
        "ld x25, 200(sp)",
        "ld x26, 208(sp)",
        "ld x27, 216(sp)",
        "ld x28, 224(sp)",
        "ld x29, 232(sp)",
        "ld x30, 240(sp)",
        "ld x31, 248(sp)",
        // restore sp
        "addi sp, sp, {trap_frame_size}",
        "sret",
        trap_frame_size = const TRAP_FRAME_SIZE,
        sepc_offset = const TRAP_FRAME_SEPC_OFFSET,
        sstatus_offset = const TRAP_FRAME_SSTATUS_OFFSET,
        scause_offset = const TRAP_FRAME_SCAUSE_OFFSET,
        stval_offset = const TRAP_FRAME_STVAL_OFFSET,
        handler = sym trap_handler,
    )
}


#[repr(C)]
pub struct TrapFrame {
    pub regs: [usize; 32],
    pub sepc: usize,
    pub sstatus: usize,
    pub scause: usize,
    pub stval: usize,
}

pub const TRAP_FRAME_SIZE: usize = core::mem::size_of::<TrapFrame>();
pub const TRAP_FRAME_SEPC_OFFSET: usize = core::mem::offset_of!(TrapFrame, sepc);
pub const TRAP_FRAME_SSTATUS_OFFSET: usize = core::mem::offset_of!(TrapFrame, sstatus);
pub const TRAP_FRAME_SCAUSE_OFFSET: usize = core::mem::offset_of!(TrapFrame, scause);
pub const TRAP_FRAME_STVAL_OFFSET: usize = core::mem::offset_of!(TrapFrame, stval);

pub fn init() {
    unsafe {
        core::arch::asm!("csrw stvec, {}", in(reg) __trap_entry as *const () as usize);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapCause {
    Exception(ExceptionCause),
    Interrupt(InterruptCause),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExceptionCause {
    InstructionAddressMisaligned,
    InstructionAccessFault,
    IllegalInstruction,
    Breakpoint,
    LoadAddressMisaligned,
    LoadAccessFault,
    StoreAddressMisaligned,
    StoreAccessFault,
    EnvironmentCallFromUMode,
    EnvironmentCallFromSMode,
    InstructionPageFault,
    LoadPageFault,
    StorePageFault,
    Unknown(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterruptCause {
    SupervisorSoftware,
    SupervisorTimer,
    SupervisorExternal,
    Unknown(usize),
}

impl TrapCause {
    pub fn from_scause(scause: usize) -> Self {
        let is_interrupt = (scause >> (usize::BITS - 1)) != 0;
        let code = scause & !(1usize << (usize::BITS - 1));

        if is_interrupt {
            Self::Interrupt(InterruptCause::from_code(code))
        } else {
            Self::Exception(ExceptionCause::from_code(code))
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Exception(cause) => cause.name(),
            Self::Interrupt(cause) => cause.name(),
        }
    }
}

impl ExceptionCause {
    fn from_code(code: usize) -> Self {
        match code {
            0 => Self::InstructionAddressMisaligned,
            1 => Self::InstructionAccessFault,
            2 => Self::IllegalInstruction,
            3 => Self::Breakpoint,
            4 => Self::LoadAddressMisaligned,
            5 => Self::LoadAccessFault,
            6 => Self::StoreAddressMisaligned,
            7 => Self::StoreAccessFault,
            8 => Self::EnvironmentCallFromUMode,
            9 => Self::EnvironmentCallFromSMode,
            12 => Self::InstructionPageFault,
            13 => Self::LoadPageFault,
            15 => Self::StorePageFault,
            _ => Self::Unknown(code),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::InstructionAddressMisaligned => "Instruction address misaligned",
            Self::InstructionAccessFault => "Instruction access fault",
            Self::IllegalInstruction => "Illegal instruction",
            Self::Breakpoint => "Breakpoint",
            Self::LoadAddressMisaligned => "Load address misaligned",
            Self::LoadAccessFault => "Load access fault",
            Self::StoreAddressMisaligned => "Store/AMO address misaligned",
            Self::StoreAccessFault => "Store/AMO access fault",
            Self::EnvironmentCallFromUMode => "Environment call from U-mode",
            Self::EnvironmentCallFromSMode => "Environment call from S-mode",
            Self::InstructionPageFault => "Instruction page fault",
            Self::LoadPageFault => "Load page fault",
            Self::StorePageFault => "Store/AMO page fault",
            Self::Unknown(_) => "Unknown exception",
        }
    }
}

impl InterruptCause {
    fn from_code(code: usize) -> Self {
        match code {
            1 => Self::SupervisorSoftware,
            5 => Self::SupervisorTimer,
            9 => Self::SupervisorExternal,
            _ => Self::Unknown(code),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::SupervisorSoftware => "Supervisor software interrupt",
            Self::SupervisorTimer => "Supervisor timer interrupt",
            Self::SupervisorExternal => "Supervisor external interrupt",
            Self::Unknown(_) => "Unknown interrupt",
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn trap_handler(tf: &mut TrapFrame) {
    let sepc = tf.sepc;
    let stval = tf.stval;
    let cause = TrapCause::from_scause(tf.scause);

    println!("trap!!");
    println!("  scause = {} ({})", cause.name(), tf.scause);
    println!("  sepc   = {:#x}", sepc);
    println!("  stval  = {:#x}", stval);

    match cause {
        TrapCause::Exception(ExceptionCause::IllegalInstruction) => {
            println!("  action = skip illegal instruction");
            tf.sepc += 4;
        }
        _ => {
            println!("  action = spin");
            loop {}
        }
    }
}
