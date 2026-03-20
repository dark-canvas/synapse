//! Pager implementation
//!
//! The kernel is mapped to 0xFFFFFF8000000000
//!   p4 index = 7f
//!   p3 index = 0
//!   p2 index = 0
//!   p1 index = 0
//!
//!   let pl4_index = (virtual_addr >> 39) & 0o777;
//!   let pl3_index = (virtual_addr >> 30) & 0o777;
//!   let pl2_index = (virtual_addr >> 21) & 0o777;
//!   let pl1_index = (virtual_addr >> 12) & 0o777;

use x86_64::registers::control::Cr3;
use x86_64::structures::paging::PhysFrame;
use x86_64::structures::paging::PageTable;
use x86_64::structures::paging::page_table::PageTableEntry;
use x86_64::PhysAddr;

use satus_struct::config::Config;
use satus_struct::module_list::ModuleList;

use crate::types::Address;
use crate::stack::{Stack, EXPAND_UP, EXPAND_DOWN};

//use log::info;

// TODO: remove this an everything that uses it and use 3 PageStack's
pub const PAGE_SIZE: usize = 4096;

pub struct PageStack {
    total_pages: usize,
}

pub enum PageType {
    Page4K,
    Page2M,
    Page1G,
}

pub struct Pager {
    pl4_table: &'static mut PageTable,
    //page_stack_4kb: Stack<'static, usize>,

    // TODO: how to represent the borrower and mapper here, as they will have to have a reference to the pager, but 
    // the pager hasn't yet been created...
    //stack_4kb PageStack::<borrower, mapper, 4*1024>,
    //stack_2mb PageStack::<borrower, mapper, 2*1024*1024>,
    //stack_1gb PageStack::<borrower, mapper, 1*1024*1024*1024>,
}
    
fn page_in_use(page: usize, page_size: usize, module_list: &ModuleList) -> bool {
    let page_start = page * page_size;
    let page_end = page_start + page_size;

    // TODO: get_module_count() instead?
    // Or add an iterator?
    for i in 0..module_list.get_num_modules() {
        let module_info = module_list.get_module_info(i).expect("Invalid module index");
        let module_start = module_info.get_start_address() as usize;
        let module_end = module_start + module_info.get_size() as usize;

        if (page_start >= module_start && page_start < module_end) || 
           (page_end > module_start && page_end <= module_end) {
            return true;
        }
    }

    false
}

impl Pager {
    pub fn new(config: &Config) -> Pager {
        let (pl4_frame, _flags) = Cr3::read();
        let pl4_addr: PhysAddr = pl4_frame.start_address();

        let module_list = ModuleList::from_page(config.get_module_list_address());

        // TOOD: need to build 2mb page stack, and 1gb page stack... ideally in a way that 
        // shared code
        // TOOD: query from bootloader struct
        let num_pages = 524196;
        // How many pages do we need to allocate in order to construct the page table 
        // itself
        let page_stack_bytes = num_pages * core::mem::size_of::<usize>();
        let page_stack_pages = (page_stack_bytes + PAGE_SIZE - 1) / PAGE_SIZE;

        // TODO: actually decide on a location for this stack...
        // default to 2mb for now... should be available...
        let mut page_stack_4kb = Stack::<usize, EXPAND_UP>::new(0x200000, page_stack_pages);
        for page in 0..num_pages {
            if ! page_in_use(page, 4096, &module_list) {
                // kernel hangs if this occurs...
                //page_stack_4kb.push(page);
            }
        }

        //info!("pl4 Physical Address: {:?}", pl4_addr);

        // TODO: detrermine number of 4kb pages (audit mapped pages or get it from bootloader?)
        // TODO: create page stacks for 4kb, 2mb and 1gb pages

        unsafe {
            let pl4_table = &mut *(pl4_frame.start_address().as_u64() as *mut PageTable);
            //Pager { pl4_table, page_stack_4kb }
            Pager { pl4_table }
        }
    }

    /*
    pub fn alloc_page(&mut self, page_type: PageType) -> Opl1ion<usize> {
        match page_type {
            PageType::Page4K => self.page_stack_4kb.pop(),
            PageType::Page2M => None, // TODO: implement
            PageType::Page1G => None, // TODO: implement
        }
    }

    pub fn free_page(&mut self, page_type: PageType, page_number: usize) {
        match page_type {
            PageType::Page4K => self.page_stack_4kb.push(page_number),
            PageType::Page2M => (), // TODO: implement
            PageType::Page1G => (), // TODO: implement
        }
    }
    */

    pub fn virtual_to_physical(&self, virtual_addr: usize) -> Option<usize> {
        let pl4_index = (virtual_addr >> 39) & 0o777;
        let pl3_index = (virtual_addr >> 30) & 0o777;
        let pl2_index = (virtual_addr >> 21) & 0o777;
        let pl1_index = (virtual_addr >> 12) & 0o777;

        unsafe {
            let pl4_entry = &self.pl4_table[pl4_index];
            if pl4_entry.is_unused() {
                return None;
            }

            // page directory entry is 4kb page...
            // TODO: this shouldn't have to be mutable, but (confirm this...)
            // the API for PageTableEntry doesn't have a way to get the address without mutably borrowing the entry
            let pl3_table = &mut *(pl4_entry.addr().as_u64() as *mut PageTable);
            let pl3_entry = &pl3_table[pl3_index];
            if pl3_entry.is_unused() {
                return None;
            }

            // this could be a 2mb page...
            let pl2_table = &mut *(pl3_entry.addr().as_u64() as *mut PageTable);
            let pl2_entry = &pl2_table[pl2_index];
            if pl2_entry.is_unused() {
                return None;
            }

            if pl2_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
                return Some(pl2_entry.addr().as_u64() as usize + (virtual_addr & 0x1FFFFF));
            }

            let pl1_table = &mut *(pl2_entry.addr().as_u64() as *mut PageTable);
            let pl1_entry = &pl1_table[pl1_index];
            if pl1_entry.is_unused() {
                return None;
            }

            Some(pl1_entry.addr().as_u64() as usize + (virtual_addr & 0xFFF))
        }
    }


    pub fn output_mmap(&self) {
        unsafe {
            for (i, entry) in self.pl4_table.iter().enumerate() {
                if !entry.is_unused() {
                    //info!("pl4 Entry {}: {:?}", i, entry);

                    let page_table = &mut *(entry.addr().as_u64() as *mut PageTable);
                    for (j, entry) in page_table.iter().enumerate() {
                        if !entry.is_unused() {
                            //info!("  Page Table Entry {}: {:?}", j, entry);

                            let page_table_2 = &mut *(entry.addr().as_u64() as *mut PageTable);
                            for (k, entry) in page_table_2.iter().enumerate() {
                                if !entry.is_unused() {
                                    //info!("    Page Table 2 Entry {}: {:?}", k, entry); 
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}