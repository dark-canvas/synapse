use x86_64::registers::control::Cr3;
use x86_64::structures::paging::PhysFrame;
use x86_64::structures::paging::PageTable;
use x86_64::structures::paging::page_table::PageTableEntry;
use x86_64::PhysAddr;

use satus_struct::config::Config;
use satus_struct::module_list::ModuleList;

use crate::types::Address;
use crate::stack::{Stack, ExpandUp, ExpandDown};

//use log::info;

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
    pml4_table: &'static mut PageTable,
    //page_stack_4kb: Stack<'static, usize>,
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
        let (pml4_frame, _flags) = Cr3::read();
        let pml4_addr: PhysAddr = pml4_frame.start_address();

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
        let mut page_stack_4kb = Stack::<usize, ExpandUp>::new(0x200000, page_stack_pages);
        for page in 0..num_pages {
            if ! page_in_use(page, 4096, &module_list) {
                // kernel hangs if this occurs...
                //page_stack_4kb.push(page);
            }
        }

        //info!("PML4 Physical Address: {:?}", pml4_addr);

        // TODO: detrermine number of 4kb pages (audit mapped pages or get it from bootloader?)
        // TODO: create page stacks for 4kb, 2mb and 1gb pages

        unsafe {
            let pml4_table = &mut *(pml4_frame.start_address().as_u64() as *mut PageTable);
            //Pager { pml4_table, page_stack_4kb }
            Pager { pml4_table }
        }
    }

    /*
    pub fn alloc_page(&mut self, page_type: PageType) -> Option<usize> {
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
        let pml4_index = (virtual_addr >> 39) & 0o777;
        let pdpt_index = (virtual_addr >> 30) & 0o777;
        let pd_index = (virtual_addr >> 21) & 0o777;
        let pt_index = (virtual_addr >> 12) & 0o777;

        unsafe {
            let pml4_entry = &self.pml4_table[pml4_index];
            if pml4_entry.is_unused() {
                return None;
            }

            // page directory entry is 4kb page...
            // TODO: this shouldn't have to be mutable, but (confirm this...)
            // the API for PageTableEntry doesn't have a way to get the address without mutably borrowing the entry
            let pdpt_table = &mut *(pml4_entry.addr().as_u64() as *mut PageTable);
            let pdpt_entry = &pdpt_table[pdpt_index];
            if pdpt_entry.is_unused() {
                return None;
            }

            // this could be a 2mb page...
            let pd_table = &mut *(pdpt_entry.addr().as_u64() as *mut PageTable);
            let pd_entry = &pd_table[pd_index];
            if pd_entry.is_unused() {
                return None;
            }

            if pd_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
                return Some(pd_entry.addr().as_u64() as usize + (virtual_addr & 0x1FFFFF));
            }

            let pt_table = &mut *(pd_entry.addr().as_u64() as *mut PageTable);
            let pt_entry = &pt_table[pt_index];
            if pt_entry.is_unused() {
                return None;
            }

            Some(pt_entry.addr().as_u64() as usize + (virtual_addr & 0xFFF))
        }
    }


    pub fn output_mmap(&self) {
        unsafe {
            for (i, entry) in self.pml4_table.iter().enumerate() {
                if !entry.is_unused() {
                    //info!("PML4 Entry {}: {:?}", i, entry);

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