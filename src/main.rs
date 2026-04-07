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

// TODO: add multiboot header, and a stub to switch to long mode and call into kernel entry

extern crate satus_struct;
use satus_struct::config::Config;
use satus_struct::module_list::ModuleList;

use pager::Pager;

const KERNEL_START: u64 = 0xFFFFFF8000000000;

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

fn get_aligned_pages_between(page_size: usize, start: Address, end: Address ) -> usize {
    // first the first 2mb aligned address within the region
    let page_size = page_size as Address;
    let first_aligned = match start & (page_size - 1) {
        i if i > 0 => start - i + page_size,
        _ => start
    };

    // find the last 2mb aligned address within the region
    let last_aligned = match end & (page_size - 1) {
        i if i > 0 => end - i,
        _ => end
    };

    ((last_aligned - first_aligned) / page_size) as usize
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

    println!("Starting Synapse...");

    let config = Config::from_page(config_addr);
    let module_list = ModuleList::from_page(config.get_module_list_address());

    println!("Module list:");
    let num_modules = module_list.get_num_modules();
    for i in 0..num_modules {
        let module = module_list.get_module_info(i).unwrap();
        let start = module.get_start_address();
        let size = module.get_size() as Address;

        println!("module {} -> 0x{:016x} - 0x{:016x} ({} bytes)", i, start, start+size, size);
    }

    println!("Creating pager...");
    let mut pager = Pager::new();
    pager.configure(&config);

    //let kernel_info = module_list.get_module_info(0).expect("No kernel module found");
    //let kernel_start = kernel_info.get_start_address();
    //let kernel_size = kernel_info.get_size();

    let framebuffer = config.get_framebuffer_address() as *mut u8;
    for i in 0..(config.get_framebuffer_size() as usize) {
        unsafe {
            *framebuffer.add(i) = 0xFF;
        }
    }

    // Need to find a way to make this work.... this complains that a second mutable borrow is occuring here, 
    // because (I believe) the `configure` call above creates references in the pager to itself and so 
    // creates a mutable borrow that is still active (active for the lifetime of the pager)
    
    //pager.alloc_page(pager::PageType::Page4K);

    let framebuffer = config.get_framebuffer_address() as *mut u8;
    for i in 0..(config.get_framebuffer_size() as usize) {
        unsafe {
            *framebuffer.add(i) = 0x80;
        }
    }

    loop {}
}
