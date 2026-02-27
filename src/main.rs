#![no_main]
#![no_std]

use core::arch::asm;
use core::panic::PanicInfo;

// TODO: add multiboot header, and a stub to switch to long mode and call into kernel entry

extern crate satus_struct;
use satus_struct::config::Config;

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let config_addr: usize;
    unsafe {
        asm!(
            "mov {var}, rax",
            var = out(reg) config_addr,
        );
    }
    let config = Config::from_page(config_addr);
    let framebuffer = config.get_framebuffer_address() as *mut u8;
    for i in 0..(config.get_framebuffer_size() as usize) {
        unsafe {
            *framebuffer.add(i) = 0xFF;
        }
    }
    loop {}
}

/*
#[entry]
fn main() -> () {
    /*
    asm!(
        "mov {var}, rax",
        var = out(reg) _,
    );
    let config = var;
    */
    loop {}
}
*/