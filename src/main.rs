#![no_main]
#![no_std]

#[cfg(test)]
extern crate std;

mod pager;

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
    let module_list = ModuleList::from_page(config.get_module_list_address());

    let mut pager = Pager::new();

    let kernel_info = module_list.get_module_info(0).expect("No kernel module found");
    let kernel_start = kernel_info.get_start_address();
    let kernel_size = kernel_info.get_size();
    //pager.map_pages(KERNEL_START, kernel_info.page_count as usize); // TODO: permissions
    //core::ptr::copy_nonoverlapping(kernel_start as *const u8, KERNEL_START as *mut u8, kernel_size);


    let framebuffer = config.get_framebuffer_address() as *mut u8;
    for i in 0..(config.get_framebuffer_size() as usize) {
        unsafe {
            *framebuffer.add(i) = 0xFF;
        }
    }
    loop {}
}
