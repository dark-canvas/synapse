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

    let mut serial = logger::SerialPort{};
    write!(serial, "Starting Synapse...\n").unwrap();

    let config = Config::from_page(config_addr);
    let module_list = ModuleList::from_page(config.get_module_list_address());

    write!(serial, "Module list:\n");
    let num_modules = module_list.get_num_modules();
    for i in 0..num_modules {
        let module = module_list.get_module_info(i).unwrap();
        let start = module.get_start_address();
        let size = module.get_size() as Address;

        write!(serial, "module {} -> 0x{:016x} - 0x{:016x} ({} bytes)\n",
            i, start, start+size, size);
    }

    let kernel_load_info = module_list.get_module_info(0).unwrap();
    let kernel_physical_start = kernel_load_info.get_start_address();
    let kernel_size = kernel_load_info.get_size();

    let mmap = MemoryMap::from_page(config.get_memory_map_address());
    let num_regions = mmap.get_num_regions();
    let (mut total_4kb_pages,
         mut total_2mb_pages,
         mut total_1gb_pages) = (0,0,0);
    write!(serial, "Available memory:\n");
    for i in 0..num_regions {
        let region = mmap.get_memory_region(i).unwrap();

        let (start, end) = region.get_address_range();

        if kernel_physical_start >= start && kernel_physical_start < end {
            write!(serial, "kernel resides in block {:016x} - {:016x}\n", start, end);
        }

        // TODO: we need to do this for allocated as well...
        if region.get_type() != MemoryRegionType::Available {
            continue;
        }

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

        write!(serial, "Region {} 0x{:016x} - {:016x} type {} ({} 4kb pages, {} 2mb pages, {} 1gb pages)\n", 
            i, start, end, region.get_type() as u8, count_4kb_pages, count_2mb_pages, count_1gb_pages).unwrap();
    }
    write!(serial, "Total: {} 4kb pages, {} 2mb pages, {} 1gb pages\n", total_4kb_pages, total_2mb_pages, total_1gb_pages);

    // now determine how many pages we'll need to map into the page stacks in order to populate them
    let mut total_pages_required = 0;
    total_pages_required += pages_required(total_4kb_pages * size_of::<Address>());
    total_pages_required += pages_required(total_2mb_pages * size_of::<Address>());
    total_pages_required += pages_required(total_1gb_pages * size_of::<Address>());

    // TODO: this is the count of pages required just to hold the page stack itself, but in order 
    // to map these pages in we'll also need to create plm3,2 & 1 tables which will also require 
    // additional 4kb pages
    write!(serial, "Creating the page stacks will require {} 4kb pages\n", total_pages_required);

    // now iterate to find the pages...
    let mut pages_found = 0;
    let required_base = kernel_physical_start + kernel_size as Address;
    // TODO: make const/global
    let mask_1gb = (pager::PAGE_SIZE_1GB-1) as Address;
    let mask_2mb = (pager::PAGE_SIZE_2MB-1) as Address;
    let size_1gb = pager::PAGE_SIZE_1GB as Address;
    let size_2mb = pager::PAGE_SIZE_2MB as Address;
    for i in 0..num_regions {
        let region = mmap.get_memory_region(i).unwrap();

        let (start, end) = region.get_address_range();
        let mut current = start;

        while current < end && pages_found != total_pages_required {
            // skip past any 1gb pages
            while current & mask_1gb == 0 && (current + size_1gb) < end {
                current += size_1gb;
            }

            // skip past any 2mb pages
            while current & mask_2mb == 0 && (current + size_2mb) < end {
                current += size_2mb;
            }

            // now select 4kb pages until we hit a 2mb aligned page (note that 1gb is also 2mb aligned)
            loop {
                current += 4096;
                if current > required_base {
                    write!(serial, "region {:3} page {:4} -> 0x{:016x}\n", i, pages_found+1, current);
                    pages_found += 1;
                }

                if current & mask_2mb == 0 || current >= end || pages_found == total_pages_required {
                    break;
                }
            } 
        }
    }

    write!(serial, "Creating pager...\n");

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
