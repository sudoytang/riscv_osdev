pub const STDOUT_FD: usize = 1;

pub const SYS_WRITE: usize = 1;
pub const SYS_EXIT: usize = 2;
pub const SYS_YIELD: usize = 3;

pub fn write(fd: usize, buf: &[u8]) -> usize {
    let ret: usize;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") SYS_WRITE,
            inlateout("a0") fd => ret,
            in("a1") buf.as_ptr() as usize,
            in("a2") buf.len(),
        );
    }
    ret
}

pub fn exit(exit_code: usize) -> ! {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") SYS_EXIT,
            in("a0") exit_code,
            options(noreturn),
        );
    }
}

pub fn yield_() {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") SYS_YIELD,
            options(nostack),
        );
    }
}
