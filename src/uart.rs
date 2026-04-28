use core::fmt::{self, Write};
use core::ptr::{read_volatile, write_volatile};

struct Uart0;

const UART0_BASE: usize = 0x1000_0000;
const UART0_LSR_OFFSET: usize = 0x5;
const UART0_LSR_TX_IDLE: u8 = 1 << 5;

fn uart0_is_writable() -> bool {
    unsafe {
        read_volatile((UART0_BASE as *const u8).add(UART0_LSR_OFFSET)) & UART0_LSR_TX_IDLE != 0
    }
}
fn uart0_putchar(ch: u8) {
    unsafe {
        while !uart0_is_writable() {}
        write_volatile(UART0_BASE as *mut u8, ch);
    }
}

fn uart0_puts(s: &str) {
    for b in s.as_bytes() {
        uart0_putchar(*b);
    }
}

impl Write for Uart0 {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        uart0_puts(s);
        Ok(())
    }
}

pub fn krnl_print(args: fmt::Arguments) {
    Uart0.write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! krnl_print {
    ($($arg:tt)*) => {
        $crate::uart::krnl_print(format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! krnl_println {
    ($($arg:tt)*) => {
        $crate::uart::krnl_print(format_args!($($arg)*));
        $crate::uart::krnl_print(format_args!("\n"));
    };
}
