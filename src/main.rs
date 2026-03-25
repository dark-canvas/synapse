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
use satus_struct::memory_map::{MemoryMap, MemoryRegion, MemoryRegionType};

use x86_64::instructions::port::Port;

use pager::Pager;
use pager::PAGE_SIZE_2MB;
use pager::pages_required;

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
    let config = Config::from_page(config_addr);
    let module_list = ModuleList::from_page(config.get_module_list_address());

    let value = 5150;
    let mut serial = logger::SerialPort{};
    write!(serial, "Hello from kernel {}\n", value).unwrap();

    let mmap = MemoryMap::from_page(config.get_memory_map_address());
    let num_regions = mmap.get_num_regions();
    let (mut total_4kb_pages,
         mut total_2mb_pages,
         mut total_1gb_pages) = (0,0,0);
    write!(serial, "Available memory:\n");
    for i in 0..num_regions {
        let region = mmap.get_memory_region(i).unwrap();

        // TODO: we need to do this for allocated as well...
        if region.get_type() != MemoryRegionType::Available {
            continue;
        }

        let (start, end) = region.get_address_range();

        let mut count_4kb_pages = (end-start) as usize / 4096;

        // determine whether there are (and how many) 2mb pages in this range
        let mut count_2mb_pages = 0;
        if end - start >=  pager::PAGE_SIZE_2MB as Address {
            count_2mb_pages = get_aligned_pages_between(pager::PAGE_SIZE_2MB, start, end);
        }

        let mut count_1gb_pages = 0;
        if count_2mb_pages >= 512 {
            count_1gb_pages = get_aligned_pages_between(pager::PAGE_SIZE_1GB, start, end);
        }

        // a page can only be one size, so if we have 2mb or 1gb aligned pages, remove the 
        // overlap from the smaller page sizes...
        count_2mb_pages -= (count_1gb_pages * (pager::PAGE_SIZE_1GB/pager::PAGE_SIZE_2MB));
        count_4kb_pages -= (count_1gb_pages * (pager::PAGE_SIZE_1GB/pager::PAGE_SIZE_4KB));
        count_4kb_pages -= (count_2mb_pages * (pager::PAGE_SIZE_2MB/pager::PAGE_SIZE_4KB));

        total_4kb_pages += count_4kb_pages;
        total_2mb_pages += count_2mb_pages;
        total_1gb_pages += count_1gb_pages;

        write!(serial, "Region 0x{:016x} - {:016x} type {} ({} 4kb pages, {} 2mb pages, {} 1gb pages)\n", 
            start, end, region.get_type() as u8, count_4kb_pages, count_2mb_pages, count_1gb_pages).unwrap();
    }
    write!(serial, "Total: {} 4kb pages, {} 2mb pages, {} 1gb pages\n", total_4kb_pages, total_2mb_pages, total_1gb_pages);

    // now determine how many pages we'll need to map into the page stacks in order to populate them
    let mut total_pages_required = 0;
    total_pages_required += pages_required(total_4kb_pages * size_of::<Address>());
    total_pages_required += pages_required(total_2mb_pages * size_of::<Address>());
    total_pages_required += pages_required(total_1gb_pages * size_of::<Address>());

    // TODO: need to...
    //  - find a way to select which 4kb pages to use
    //  - map them into the page stacks
    //  - account for them no longer being available (so they dont go on the available stack)
    //  - ensure they're added to the allocated stack

    write!(serial, "Creating the page stacks will require {} 4kb pages\n", total_pages_required);

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
