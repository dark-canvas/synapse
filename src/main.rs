#![no_std]
#![cfg_attr(not(test), no_main)]

#[cfg(test)]
extern crate std;

#[macro_use]
mod logger;
mod pager;
mod page_stack;
mod stack;
mod types;

use types::Address;

use core::arch::asm;
use core::panic::PanicInfo;
use core::fmt::Write;

// TODO: add multiboot header, and a stub to switch to long mode and call into kernel entry

extern crate satus_struct;
use satus_struct::config::Config;
use satus_struct::module_list::ModuleList;
use satus_struct::memory_map::{MemoryMap, MemoryRegion};

use x86_64::instructions::port::Port;

use pager::Pager;

const KERNEL_START: u64 = 0xFFFFFF8000000000;

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[cfg(not(test))]
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let config_addr: Address;
    unsafe {
        asm!(
            "mov {var}, rax",
            var = out(reg) config_addr,
        );
    }
    let config = Config::from_page(config_addr);
    let module_list = ModuleList::from_page(config.get_module_list_address());

    let value = 5150;
    let mut serial = logger::SerialPort{};
    write!(serial, "Hello from kernel {}\n", value).unwrap();

    let mmap = MemoryMap::from_page(config.get_memory_map_address());
    let num_regions = mmap.get_num_regions();
    for i in 0..num_regions {
        let region = mmap.get_memory_region(i).unwrap();
        let (start, end) = region.get_address_range();
        write!(serial, "Region {:x} - {:x} type {}\n", start, end, region.get_type() as u8).unwrap();

    }

    let mut pager = Pager::new();
    pager.configure(&config);

    let kernel_info = module_list.get_module_info(0).expect("No kernel module found");
    let kernel_start = kernel_info.get_start_address();
    let kernel_size = kernel_info.get_size();

    let framebuffer = config.get_framebuffer_address() as *mut u8;
    for i in 0..(config.get_framebuffer_size() as usize) {
        unsafe {
            *framebuffer.add(i) = 0xFF;
        }
    }
    loop {}
}
