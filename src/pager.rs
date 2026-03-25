//! Pager implementation
//!
//! The kernel is mapped to 0xFFFFFF8000000000 (last element in pl4 table)
//!   p4 index = 511
//!   p3 index = 0
//!   p2 index = 0
//!   p1 index = 0
//! 
//! Physical memory is "identity" mapped into pl4[510] == 0xFFFFFF0000000000
//! Can just copy pl4[0] to pl4[510]?
//!
//! This results in support for up to 512GB of physical memory, which equates to:
//!   - 134217728 4kb pages (2^27) == 1073741824 (1gb) required for the page stack
//!   - 262144 2mb pages (2^18)    == 2097152 (2mb) required for the page stack
//!   - 512 1gb pages (2^9)        == 4096 requires for the page stack
//!
//! Which can be mapped to the top of the virtual address space (above the kernel)
//!   - 0xFFFFFFD000000000 - 0xFFFFFFE000000000 -> 4kb page stack
//!   - 0xFFFFFFE000000000 - 0xFFFFFFE000200000 -> 2mb page stack
//!   - 0xFFFFFFE000200000 - 0xFFFFFFE000201000 -> 1gb page stack

use x86_64::registers::control::Cr3;
use x86_64::structures::paging::PhysFrame;
use x86_64::structures::paging::PageTable;
use x86_64::structures::paging::PageTableFlags;
use x86_64::structures::paging::page_table::PageTableEntry;
use x86_64::PhysAddr;

use satus_struct::config::Config;
use satus_struct::module_list::ModuleList;
use satus_struct::memory_map::{MemoryMap, MemoryRegion};

use crate::types::Address;
use crate::stack::{Stack, EXPAND_UP, EXPAND_DOWN};
use crate::page_stack::{PageStack, PageBorrower, PageMapper};

//use log::info;

pub const PAGE_SIZE_4KB: usize = 4*1096;
pub const PAGE_SIZE_2MB: usize = 2*1024*1024;
pub const PAGE_SIZE_1GB: usize = 1*1024*1024*1024;

const PHYSICAL_OFFSET: Address = 0xFFFFFF0000000000;

const PAGE_STACK_4KB_BASE: Address = 0xFFFFFFD000000000;
const PAGE_STACK_2MB_BASE: Address = 0xFFFFFFE000000000;
const PAGE_STACK_1GB_BASE: Address = 0xFFFFFFE000200000;

const PAGE_STACK_4KB_MAX_PAGES: usize = 134217728;
const PAGE_STACK_2MB_MAX_PAGES: usize = 262144;
const PAGE_STACK_1GB_MAX_PAGES: usize = 512;

pub enum PageType {
    Page4K,
    Page2M,
    Page1G,
}

/*
struct BorrowsFrom<T> {
    stack: &T,
}

impl<T> BorrowsFrom<T> {
    fn new(source: &T) -> Self {
        BorrowsFrom {
            stack: source,
        }
    }
}

impl<T> PageBorrower for BorrowsFrom<T> {
    fn borrow_pages(&self) -> Option< (Address, usize) > {
        /*
        match self.stack.allocate_page() {
            Ok(address) => Ok( (Address, 1) ),
            None => None
        }
        */
        None
    }
}
*/

#[derive(Clone)]
struct StubMapper {}

impl PageMapper for StubMapper {
    fn ensure_mapped(&self, base: Address, end: Address) -> Result<bool,()> {
        Err(())
    }
}

pub struct Pager<'a> {
    pl4_table: &'static mut PageTable,

    // TODO: how to represent the borrower and mapper here, as they will have to have a reference to the pager, but 
    // the pager hasn't yet been created...
    stack_1gb: PageStack::<'a, StubMapper, PAGE_SIZE_1GB>,
    stack_2mb: PageStack::<'a, StubMapper, PAGE_SIZE_2MB>,
    stack_4kb: PageStack::<'a, StubMapper, PAGE_SIZE_4KB>,
}

// TODO: probably doesn't need to be public
pub fn pages_required(size: usize) -> usize {
    (size + PAGE_SIZE_4KB) / PAGE_SIZE_4KB
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

impl<'a> Pager<'a> {
    /// Returns an instance of Pager with any non-specific configuration done.
    /// System-specific configuration must be performed first (via the configure method)
    /// before this pager can actually be used.
    /// I would prefer to do all the configuration here, but the page stacks refer to 
    /// each other, and attempting to set all that up in here will result in Rust disallowing 
    /// it, as it claims the return value (pager) has references to variables that only 
    /// exist in the scope of htis method.  While not technically true, I'm not sure how to 
    /// avoid this (and I'm not sure if Rust will actually "move" the return value, or 
    /// construct it in place once on the stack... the former is problematic if there are 
    /// embedded pointers)
    pub fn new() -> Self {
        let (pl4_frame, _flags) = Cr3::read();
        let pl4_addr: PhysAddr = pl4_frame.start_address();

        let mapper = StubMapper{};
        // TODO: detrermine number of 4kb pages (audit mapped pages or get it from bootloader?)
        // TODO: create page stacks for 4kb, 2mb and 1gb pages

        unsafe {
            let pl4_table = &mut *(pl4_frame.start_address().as_u64() as *mut PageTable);

            // map into itself for easier virtual to physical mappings
            let pl4_entry = &mut pl4_table[510];
            pl4_entry.set_addr(pl4_addr, PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE );
            // Or... map all of physical memory, identity mapped into the second last pl4 entry

            Pager { 
                pl4_table: pl4_table,  // Make this an address instead?
                stack_1gb: PageStack::<_, PAGE_SIZE_1GB>::new(mapper.clone(), PAGE_STACK_1GB_BASE, PAGE_STACK_1GB_MAX_PAGES),
                stack_2mb: PageStack::<_, PAGE_SIZE_2MB>::new(mapper.clone(), PAGE_STACK_2MB_BASE, PAGE_STACK_2MB_MAX_PAGES),
                stack_4kb: PageStack::<_, PAGE_SIZE_4KB>::new(mapper.clone(), PAGE_STACK_4KB_BASE, PAGE_STACK_4KB_MAX_PAGES),
            }
        }
    }

    pub fn configure(&'a mut self, config: &Config) {
        let module_list = ModuleList::from_page(config.get_module_list_address());
        let mmap = MemoryMap::from_page(config.get_memory_map_address());

        self.stack_2mb.set_borrow_source(&self.stack_1gb);
        self.stack_4kb.set_borrow_source(&self.stack_2mb);

        // we're abour to populate the page stacks with all the avilable pages, but they're 
        // current not mapped at all.
        // The act of mapping pages to form the stacks will consume pages in order to create 
        // l2, 3, and 4 tables, so we need some algorithm to select those pages from 
        // the count of free pages

        // Need to push all available pages onto the stacks, preferring to push the largest (1gb) pages first
        // Need to know how much memory exists
        // How?
        // Once that's known, we need to expicitly map enough pages to populate the 1GB stack,
        // Then, if anything remains, we need to map enough pages to populate the 2MB stack,
        // Then, if anything remains, we need to map enough pages to populate the 4KB stack.
        // Worst case scenario means we need 1 4kb page for every 512*4096 == 2mb of physical memory
        // To do this, we need a way to allocate free 4kb pages... which is tricky, as this whole 
        // thing we're coding is meant to do exactly that... there's a chicken and egg scenario.
        // Explicitly create the stacks as if everything is available?
        // And then allocate various pages after?

        // After this is all setup, then physical memory can be identity mapped to PHYSICAL_OFFSET
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