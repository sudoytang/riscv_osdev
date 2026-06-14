//! Read-only program image store.
//!
//! The embedded `user.bin` flat binary lives in the kernel `.user_bin` section
//! and acts as a linear in-memory program repository. Each `spawn` copies the
//! image into freshly allocated physical pages rather than mapping the embedded
//! copy directly into user address space.

unsafe extern "C" {
    static user_bin_start: [u8; 0];
    static user_bin_end: [u8; 0];
}

pub const USER_CODE_VA: usize = 0x4000_0000;
pub const USER_STACK_VA: usize = 0x4001_0000;
pub const USER_STACK_SIZE: usize = 4096;
pub const USER_STACK_TOP: usize = USER_STACK_VA + USER_STACK_SIZE;

/// Maximum number of 4KB pages a single user program image may occupy.
pub const MAX_USER_CODE_PAGES: usize = 16;

/// A program entry describes one runnable entry point inside the embedded image.
#[derive(Clone, Copy, Debug)]
pub struct ProgramEntry {
    pub name: &'static str,
    pub entry_offset: usize,
}

pub const USER_MAIN_0: ProgramEntry = ProgramEntry {
    name: "user_main_0",
    entry_offset: 0,
};

pub const USER_MAIN_1: ProgramEntry = ProgramEntry {
    name: "user_main_1",
    entry_offset: 4,
};

impl ProgramEntry {
    pub fn entry_va(self) -> usize {
        USER_CODE_VA + self.entry_offset
    }
}

/// Returns the embedded flat binary as a read-only byte slice.
pub fn embedded_user_bin() -> &'static [u8] {
    unsafe {
        let start = core::ptr::addr_of!(user_bin_start) as *const u8;
        let end = core::ptr::addr_of!(user_bin_end) as *const u8;
        core::slice::from_raw_parts(start, end.offset_from(start) as usize)
    }
}
