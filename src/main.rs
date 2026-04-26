#![no_std]
#![cfg_attr(not(test), no_main)]
#![feature(generic_const_exprs)]

#[cfg(test)]
extern crate std;

#[macro_use]
mod logger;
mod pager;
mod stack;
mod types;

use types::Address;

use core::arch::asm;
use core::panic::PanicInfo;

extern crate satus_struct;
use satus_struct::config::Config;
use satus_struct::module_list::ModuleList;

use pager::Pager;

const KERNEL_START: u64 = 0xFFFFFF8000000000;
const KERNEL_STACK_SIZE: u64 = 2*1024*1024; // This is completely arbitraty...

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("Panic: {}", info);
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
    let pager = Pager::new(&config);

    //let kernel_info = module_list.get_module_info(0).expect("No kernel module found");
    //let kernel_start = kernel_info.get_start_address();
    //let kernel_size = kernel_info.get_size();

    let framebuffer = config.get_framebuffer_address() as *mut u8;
    for i in 0..(config.get_framebuffer_size() as usize) {
        unsafe {
            *framebuffer.add(i) = 0xFF;
        }
    }

    pager::run_time_tests(&pager);

    let framebuffer = config.get_framebuffer_address() as *mut u8;
    for i in 0..(config.get_framebuffer_size() as usize) {
        unsafe {
            *framebuffer.add(i) = 0x80;
        }
    }

    loop {}
}
