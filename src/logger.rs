use core::fmt::{self, Write};
use x86_64::instructions::port::Port;

use crate::types::Address;

pub struct SerialPort {}

pub const LOG_ALLOC_4KB: &str = "alloc_4kb";
pub const LOG_ALLOC_2MB: &str = "alloc_2mb";
pub const LOG_ALLOC_1GB: &str = "alloc_1gb";
pub const LOG_FREE_4KB: &str = "free_4kb";
pub const LOG_FREE_2MB: &str = "free_2mb";
pub const LOG_FREE_1GB: &str = "free_1gb";
pub const LOG_BORROW_2MB: &str = "borrow_2mb";
pub const LOG_BORROW_1GB: &str = "borrow_1gb";
pub const LOG_AGGREGATE_2MB: &str = "aggregate_2mb";
pub const LOG_AGGREGATE_1GB: &str = "aggregate_1gb";


pub struct FrameBufferLogger {
    address: Address,
    width: usize,
    height: usize,
    bytes_per_line: usize,

    current_x: usize,
    current_y: usize,

    enabled: bool,
}

impl FrameBufferLogger {
    pub fn new(address: Address, width: usize, height: usize, bytes_per_line: usize) -> Self {
        FrameBufferLogger {
            address,
            width,
            height,
            bytes_per_line,
            current_x: 0,
            current_y: 0,
            enabled: true,
        }
    }

    pub fn disable(mut self) -> Self {
        self.enabled = false;
        self
    }
}

impl Write for FrameBufferLogger {  
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if !self.enabled {
            return Ok(());
        }
        let square_size = 10;
        let color = match(s) {
            LOG_ALLOC_4KB => 0x00FF0000, // Red
            LOG_ALLOC_2MB => 0x0000FF00, // Green
            LOG_ALLOC_1GB => 0x000000FF, // Blue
            LOG_FREE_4KB => 0x00FF8080, // Light Red
            LOG_FREE_2MB => 0x0080FF80, // Light Green
            LOG_FREE_1GB => 0x008080FF, // Light Blue
            LOG_BORROW_2MB => 0x00FFFF00, // Yellow
            LOG_BORROW_1GB => 0x00FF00FF, // Magenta
            LOG_AGGREGATE_2MB => 0x0000FFFF, // Cyan
            LOG_AGGREGATE_1GB => 0x00008080, // Darker Cyan
            _ => 0x00808080, // Grey for unknown operations
        };
           
        for y in 0..square_size {
            let mut offset = ((self.current_y + y) * self.bytes_per_line) + (self.current_x * 4);
            for x in 0..square_size {
                unsafe {
                    core::ptr::write_volatile((self.address + offset as Address) as *mut u32, color);
                }
                offset += 4; // Move to the next pixel (assuming 32 bits per pixel)
            }
        }

        self.current_x += square_size;
        if self.current_x >= self.width {
            self.current_x = 0;
            self.current_y += square_size;
        }

        if self.current_y >= self.height {
            // copy everything up...
            unsafe {
                core::ptr::copy(
                    (self.address + (square_size * self.bytes_per_line) as Address) as *const u8, 
                    self.address as *mut u8, 
                    ((self.height - square_size) * self.bytes_per_line) as usize);
            }
            self.current_y -= square_size; // Reset to the top if we exceed the height
        }

        // Implementation for writing to frame buffer
        Ok(())
    }
}

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
