use core::fmt::{self, Write};
use x86_64::instructions::port::Port;

pub struct SerialPort {}

impl Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        unsafe {
            // TODO: just rewrite in assembly?
            let mut port = Port::new(0x3F8);
            for byte in s.bytes() {
                port.write(byte);
            }
        }
        Ok(())
    }
}



#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::logger::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[cfg(not(test))]
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    let mut serial_port = SerialPort {};
    serial_port.write_fmt(args).unwrap();
}

#[cfg(test)]
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    // Nothing for now...
}
