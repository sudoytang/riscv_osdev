use crate::{krnl_print, krnl_println};

#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.trap")]
pub unsafe extern "C" fn __trap_entry() {
    core::arch::naked_asm!(
        // Swap sp with sscratch to detect whether the trap came from U-mode.
        // S-mode convention: sscratch = 0.
        // U-mode convention: sscratch = kernel trap stack top.
        "csrrw sp, sscratch, sp",
        "bnez sp, .Lfrom_u_mode",

        // From S-mode: sp is now 0, and sscratch has the original kernel sp.
        // Swap back so we keep handling the trap on the current kernel stack.
        "csrrw sp, sscratch, sp",

        // U-mode arrives here with sp already switched to kernel trap stack top.
        // S-mode arrives here after swapping back to the original kernel sp.
        ".Lfrom_u_mode:",
        "addi sp, sp, -{trap_frame_size}",

        // Save general-purpose registers before using any of them as temporaries.
        // x0 is always zero, so there is no need to save it.
        "sd x1, 8(sp)",
        // x2 is the interrupted sp. Save it after all other registers are safe.
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

        // Now x10/a0 is saved, so it is safe to use as a temporary.
        // If sscratch is non-zero, it holds the interrupted user sp.
        // Otherwise this was an S-mode trap, and the interrupted sp is
        // the current trap frame base plus the trap frame size.
        "csrr a0, sscratch",    // <- a0 = user sp if in U-mode, 0 if in S-mode
        "bnez a0, .Lsave_interrupted_sp",
        "addi a0, sp, {trap_frame_size}",  // <- a0 = sp + sizeof(TrapFrame) if in S-mode
        ".Lsave_interrupted_sp:",
        "sd a0, 16(sp)",

        // Save trap CSRs.
        "csrr a0, sepc",
        "sd a0, {sepc_offset}(sp)",
        "csrr a0, sstatus",
        "sd a0, {sstatus_offset}(sp)",
        "csrr a0, scause",
        "sd a0, {scause_offset}(sp)",
        "csrr a0, stval",
        "sd a0, {stval_offset}(sp)",

        // Pass &mut TrapFrame as the first argument.
        "mv a0, sp",
        "call {handler}",

        // Restore trap CSRs. The handler may have modified sepc.
        "ld a0, {sepc_offset}(sp)",
        "csrw sepc, a0",
        "ld a0, {sstatus_offset}(sp)",
        "csrw sstatus, a0",

        // Restore general-purpose registers. Keep x5/t0 for last so we can
        // use it to decide whether to switch back to the user stack.
        "ld x1, 8(sp)",
        "ld x3, 24(sp)",
        "ld x4, 32(sp)",
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

        // If sscratch is non-zero, it holds the user sp and we must switch
        // back to it before sret. For S-mode traps, sscratch is zero.
        "csrr x5, sscratch",
        "bnez x5, .Lreturn_to_user",

        // Return to S-mode: restore t0, release the trap frame, and sret.
        "ld x5, 40(sp)",
        "addi sp, sp, {trap_frame_size}",
        "sret",

        // Return to U-mode: restore t0, release the kernel trap frame, then
        // swap sp with sscratch so sp becomes the interrupted user sp and
        // sscratch becomes the kernel trap stack top again.
        ".Lreturn_to_user:",
        "ld x5, 40(sp)",
        "addi sp, sp, {trap_frame_size}",
        "csrrw sp, sscratch, sp",
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

macro_rules! register_indices {
    ($($name:ident = $value:expr;)+) => {
        $(
            #[allow(dead_code)]
            pub const $name: usize = $value;
        )+
    };
}

register_indices! {
    ZERO = 0;
    RA = 1;
    SP = 2;
    GP = 3;
    TP = 4;
    T0 = 5;
    T1 = 6;
    T2 = 7;
    S0 = 8;
    FP = 8;
    S1 = 9;
    A0 = 10;
    A1 = 11;
    A2 = 12;
    A3 = 13;
    A4 = 14;
    A5 = 15;
    A6 = 16;
    A7 = 17;
    S2 = 18;
    S3 = 19;
    S4 = 20;
    S5 = 21;
    S6 = 22;
    S7 = 23;
    S8 = 24;
    S9 = 25;
    S10 = 26;
    S11 = 27;
    T3 = 28;
    T4 = 29;
    T5 = 30;
    T6 = 31;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Syscall {
    Reserved,
    PutChar,
    Exit,
    Unknown(usize),
}

impl Syscall {
    pub const SYS_PUTCHAR: usize = Self::PutChar.into_usize();
    pub const SYS_EXIT: usize = Self::Exit.into_usize();

    const fn into_usize(self) -> usize {
        match self {
            Self::Reserved => 0,
            Self::PutChar => 1,
            Self::Exit => 2,
            Self::Unknown(n) => n,
        }
    }

    fn from_number(number: usize) -> Self {
        match number {
            0 => Self::Reserved,
            Self::SYS_PUTCHAR => Self::PutChar,
            Self::SYS_EXIT => Self::Exit,
            _ => Self::Unknown(number),
        }
    }
}

impl Syscall {
    pub fn krnl_handle(self, tf: &mut TrapFrame) {
        match self {
            Self::Reserved => {
                krnl_println!("  syscall 0 reserved");
                tf.regs[A0] = usize::MAX;
            }
            Self::PutChar => {
                let ch = tf.regs[A0] as u8;
                krnl_print!("{}", ch as char);
                tf.regs[A0] = 0;
            }
            Self::Exit => {
                let exit_code = tf.regs[A0];
                krnl_println!("  User exited with code {}", exit_code);
                krnl_println!("  Kernel halted");
                loop {}
            }
            Self::Unknown(n) => {
                krnl_println!("  syscall {} not implemented", n);
                tf.regs[A0] = usize::MAX;
            }
        }
    }
}

const TRACE_SYSCALLS: bool = false;



#[unsafe(no_mangle)]
pub extern "C" fn trap_handler(tf: &mut TrapFrame) {
    let sepc = tf.sepc;
    let stval = tf.stval;
    let cause = TrapCause::from_scause(tf.scause);

    let should_log_trap = !matches!(
        cause,
        TrapCause::Exception(ExceptionCause::EnvironmentCallFromUMode)
    ) || TRACE_SYSCALLS;

    if should_log_trap {
        krnl_println!("trap!!");
        krnl_println!("  scause = {} ({})", cause.name(), tf.scause);
        krnl_println!("  sepc   = {:#x}", sepc);
        krnl_println!("  stval  = {:#x}", stval);
    }



    match cause {
        TrapCause::Exception(ExceptionCause::IllegalInstruction) => {
            krnl_println!("  action = skip illegal instruction");
            tf.sepc += 4;
        }
        TrapCause::Exception(ExceptionCause::EnvironmentCallFromUMode) => {
            if should_log_trap {
                krnl_println!("  action = handle ecall");
            }
            let syscall_num = tf.regs[A7];
            let syscall = Syscall::from_number(syscall_num);
            syscall.krnl_handle(tf);
            tf.sepc += 4;
        }
        _ => {
            krnl_println!("  action = spin");
            loop {}
        }
    }
}
