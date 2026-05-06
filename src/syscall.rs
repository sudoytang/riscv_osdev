pub const STDOUT_FD: usize = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Syscall {
    Reserved,
    Write,
    Exit,
    Yield,
    Unknown(usize),
}

impl Syscall {
    pub const SYS_WRITE: usize = Self::Write.into_usize();
    pub const SYS_EXIT: usize = Self::Exit.into_usize();
    pub const SYS_YIELD: usize = Self::Yield.into_usize();

    const fn into_usize(self) -> usize {
        match self {
            Self::Reserved => 0,
            Self::Write => 1,
            Self::Exit => 2,
            Self::Yield => 3,
            Self::Unknown(n) => n,
        }
    }

    pub fn from_number(number: usize) -> Self {
        match number {
            0 => Self::Reserved,
            Self::SYS_WRITE => Self::Write,
            Self::SYS_EXIT => Self::Exit,
            Self::SYS_YIELD => Self::Yield,
            _ => Self::Unknown(number),
        }
    }
}
