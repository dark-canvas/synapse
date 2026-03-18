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
