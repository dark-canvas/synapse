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

use core::ops::Index; 
use core::ptr;

use x86_64::registers::control::Cr3;
use x86_64::structures::paging::Size4KiB;
use x86_64::structures::paging::PhysFrame;
use x86_64::structures::paging::PageTable;
use x86_64::structures::paging::PageTableFlags;
use x86_64::PhysAddr;

use satus_struct::config::Config;
use satus_struct::module_list::ModuleList;
use satus_struct::memory_map::{MemoryMap, MemoryRegionType};

use crate::types::Address;
use crate::stack::SimpleStack;
use crate::page_stack::{PageStack, PageMapper};

//use log::info;

pub const PAGE_SIZE_4KB: usize = 4*1024;
pub const PAGE_SIZE_2MB: usize = 2*1024*1024;
pub const PAGE_SIZE_1GB: usize = 1*1024*1024*1024;

pub const PAGE_MASK_4KB: Address = 4*1024-1;
pub const PAGE_MASK_2MB: Address = 2*1024*1024-1;
pub const PAGE_MASK_1GB: Address = 1*1024*1024*1024-1;

const PHYSICAL_OFFSET: Address = 0xFFFFFF0000000000;

const PAGE_STACK_4KB_BASE: Address = 0xFFFFFFD000000000;
const PAGE_STACK_2MB_BASE: Address = 0xFFFFFFE000000000;
const PAGE_STACK_1GB_BASE: Address = 0xFFFFFFE000200000;

const PAGE_STACK_4KB_MAX_PAGES: usize = 134217728;
const PAGE_STACK_2MB_MAX_PAGES: usize = 262144;
const PAGE_STACK_1GB_MAX_PAGES: usize = 512;

#[derive(Copy, Clone)]
pub struct PhysicalAddress(Address);
#[derive(Copy, Clone)]
pub struct VirtualAddress(Address);

pub enum PageType {
    Page4K,
    Page2M,
    Page1G,
}

type PageAllocator = fn() -> Result< Address, &'static str>;
// Not sure if the error string is actually useful here
type CreatePageTable = fn() -> Result< PhysFrame::<Size4KiB>, &'static str>;

// TODO: move to a common/helper module?
// TODO: these are now "is_x_page_aligned" functions
// TODO: add actual is_x_page function
fn is_1gb_page(addr: Address) -> bool {
    addr & PAGE_MASK_1GB == 0
}

fn is_2mb_page(addr: Address) -> bool {
    addr & PAGE_MASK_2MB == 0 //&& !is_1gb_page(addr)
}

fn is_4kb_page(addr: Address) -> bool {
    addr & PAGE_MASK_4KB == 0 //&& !is_2mb_page(addr) && !is_1gb_page(addr)
}

fn next_1gb_page(addr: Address) -> Address {
    (addr + PAGE_SIZE_1GB as Address) & !PAGE_MASK_1GB
}

fn next_2mb_page(addr: Address) -> Address {
    (addr + PAGE_SIZE_2MB as Address) & !PAGE_MASK_2MB
}

fn next_4kb_page(addr: Address) -> Address {
    (addr + PAGE_SIZE_4KB as Address) & !PAGE_MASK_4KB
}

// TODO: need an implementation of this for getting 4kb, 2mb and 1gb available pages 
// from the mmap structure
// Remember to remove the pages which we consumed trying to map the page stacks
struct PageIterator<'a> {
    mmap: &'a MemoryMap,
    page_size: Address,
    current_region: usize,
    current_page: Option<Address>,
    region_type: Option<MemoryRegionType>,
    base_address: Option<Address>,
    exclude_page_range: Option< (Address, Address) >
}

impl<'a> PageIterator<'a> {
    pub fn new(mmap: &'a MemoryMap, page_size: usize) -> Self {
        PageIterator{
            mmap,
            page_size: page_size as Address,
            current_region: 0,
            current_page: None,
            region_type: None,
            base_address: None,
            exclude_page_range: None,
        }
    }

    pub fn with_region_type(mut self, region_type: MemoryRegionType) -> Self {
        self.region_type = Some(region_type);
        self
    }

    pub fn with_base_address(mut self, base_address: Address) -> Self {
        self.base_address = Some(base_address);
        self
    }

    pub fn excluding_range(mut self, start: Address, end: Address) -> Self {
        self.exclude_page_range = Some((start, end));
        self
    }

    fn passes_filters(&self, addr: Address) -> bool {
        if let Some(base_address) = self.base_address {
            if addr < base_address {
                return false;
            }
        }

        if let Some(exclude_page_range) = self.exclude_page_range {
            if addr >= exclude_page_range.0 && addr < exclude_page_range.1 {
                return false;
            }
        }

        true
    }

    pub fn get_count(&self) -> usize {
        // iterate all the pages, without editing our internal state
        let mut current_region = 0;
        let mut current_page : Option<Address> = None;
        let mut count = 0;
        loop {
            let (region, page, result) = self.iterate(current_region, current_page);
            if result == None {
                break;
            }
            count += 1;
            current_region = region;
            current_page = Some(page);
        }
        count
    }

    pub fn get_current(&self) -> Option<Address> {
        self.current_page
    }

    fn iterate(&self, current_region: usize, start_page: Option<Address>) -> (usize, Address, Option<Address>) {
        let mut start_page = start_page;
        let num_regions = self.mmap.get_num_regions();

        //println!("Iterating page stack for page size 0x{:x}, starting at region {}, page 0x{:x?}", self.page_size, current_region, start_page);

        for i in current_region..num_regions {
            //current_region = i;
            let region = self.mmap.get_memory_region(i).unwrap();

            if let Some(region_type) = self.region_type {
                if region.get_type() != region_type {
                    continue;
                }
            }

            let (start, end) = region.get_address_range();
            let mut current = match start_page {
                Some(sp) => { start_page = None; sp },
                None => start,
            };
        
            // if a base address was specified, skip past any regions before it
            if let Some(base_address) = self.base_address {
                if end < base_address {
                    continue;
                }
            }

            while current < end  && current + PAGE_SIZE_4KB as Address <= end {
                // skip past any 1gb pages
                while is_1gb_page(current) && next_1gb_page(current) <= end {
                    let next = next_1gb_page(current);
                    if self.page_size == PAGE_SIZE_1GB as Address && self.passes_filters(current) {
                        //println!("  Returning 0x{:x}", current);
                        return (i, next, Some(current));
                    }
                    current = next;
                }

                // skip past any 2mb pages
                // TODO: if we find a 1GB aligned page we must break and restart...
                while is_2mb_page(current) && next_2mb_page(current) <= end {
                    if is_1gb_page(current) && next_1gb_page(current) <= end {
                        break;
                    }

                    let next = next_2mb_page(current);
                    if self.page_size == PAGE_SIZE_2MB as Address && self.passes_filters(current) {
                        //println!("  Returning 0x{:x}", current);
                        return (i, next, Some(current));
                    }
                    current = next;
                }

                while is_4kb_page(current) && next_4kb_page(current) <= end {
                    if is_2mb_page(current) && next_2mb_page(current) <= end {
                        break;
                    }

                    let next = next_4kb_page(current);
                    if self.page_size == PAGE_SIZE_4KB as Address && self.passes_filters(current){
                        //println!("  Returning 0x{:x}", current);
                        return (i, next, Some(current));
                    }
                    current = next;
                }


                // TODO: Can this be made the same as the above style loops?
                // If so... can it be a macro, or a templated function?
                // now select 4kb pages until we hit a 2mb aligned page (note that 1gb is also 2mb aligned)
                /*
                loop {
                    let next = current + PAGE_SIZE_4KB as Address;

                    if self.page_size == PAGE_SIZE_4KB as Address && next <= end {
                        println!("  Returning {}", current);
                        return (i, next, Some(current));
                    }

                    current = next;

                    // if we hit a 2mb aligned page, break out of the loop to see if it's a full 2mb page
                    if current & PAGE_MASK_2MB == 0 || current >= end {
                        break;
                    }

                }*/
            }
        }
        //println!("  Returning None");
        return (0, 0, None);
    }
}

// 2. Implement the Iterator trait
impl<'a> Iterator for PageIterator<'a> {
    // Specify the type of item the iterator yields
    type Item = Address;

    fn next(&mut self) -> Option<Self::Item> {
        let (region, page, result) = self.iterate(self.current_region, self.current_page);
        self.current_region = region;
        self.current_page = Some(page);
        result
    }
}

// TODO: remove?
#[derive(Clone)]
struct StubMapper {}

impl PageMapper for StubMapper {
    fn ensure_mapped(&self, base: Address, end: Address) -> Result<bool,()> {
        Err(())
    }
}

pub struct Pager<'a> {
    //pl4_table: &'static mut PageTable,

    /// Must return a zero'd out 4kb physical page address
    //create_page_table: GetPhysicalPage,

    // TODO: how to represent the borrower and mapper here, as they will have to have a reference to the pager, but 
    // the pager hasn't yet been created...
    stack_1gb: PageStack::<'a, StubMapper, PAGE_SIZE_1GB>,
    stack_2mb: PageStack::<'a, StubMapper, PAGE_SIZE_2MB>,
    stack_4kb: PageStack::<'a, StubMapper, PAGE_SIZE_4KB>,
}

// TODO: probably doesn't need to be public
pub fn pages_required(size: usize) -> usize {
    (size + (PAGE_SIZE_4KB - 1)) / PAGE_SIZE_4KB
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
                //pl4_table: pl4_table,  // Make this an address instead?
                stack_1gb: PageStack::<_, PAGE_SIZE_1GB>::new(mapper.clone(), PAGE_STACK_1GB_BASE, PAGE_STACK_1GB_MAX_PAGES),
                stack_2mb: PageStack::<_, PAGE_SIZE_2MB>::new(mapper.clone(), PAGE_STACK_2MB_BASE, PAGE_STACK_2MB_MAX_PAGES),
                stack_4kb: PageStack::<_, PAGE_SIZE_4KB>::new(mapper.clone(), PAGE_STACK_4KB_BASE, PAGE_STACK_4KB_MAX_PAGES),
            }
        }
    }

    // might need to template this function?
    // PageIterator could be a trait, or a concrete class with different data?
    // TODO: this only works for expand up stacks...
    fn create_page_stack<T, F>(&self, stack: &mut T, pages: PageIterator, new_page: &mut F) 
        where T : SimpleStack<Address> + Index<usize>,
              F : FnMut() -> Option< Address > {

        // TODO: this doeesn't seem to be returning the base of the stack, it's returning the top!?
        // Confirm what's going on here.
        let stack_base_address = (ptr::addr_of!(stack[0]) as *const Address) as Address;
        let pages_count = pages.get_count();
        let required_stack_size_in_pages = pages_required(pages_count * size_of::<Address>());
        println!("Page stack contains {} addresses, consuming {} pages for the stack structure", pages_count, required_stack_size_in_pages);        

        for i in 0..required_stack_size_in_pages {
            println!("Mapping page {}", i);
            self._map_physical_to_virtual(
                PhysicalAddress((new_page)().expect("Unable to map page stack")), 
                VirtualAddress(stack_base_address + (i*PAGE_SIZE_4KB) as Address),
                PageTableFlags::WRITABLE,
                || {
                    println!("Acquiring new page for page stack: {}", i);
                    (new_page)().map(|addr| {
                        unsafe {  
                            core::ptr::write_bytes(addr as *mut u8, 0, 0x1000);
                            PhysFrame::<Size4KiB>::from_start_address_unchecked(PhysAddr::new(addr))
                        }
                    })
                }
            ).expect("Unable to map");
        }
    }

    fn populate_page_stack<T>(&self, stack: &mut T, pages: PageIterator) 
        where T : SimpleStack<Address> + Index<usize> {
        println!("Populating stack with pages...");
        for page in pages {
            stack.push(page);
        }
    }

    pub fn configure(&'a mut self, config: &Config) {
        let module_list = ModuleList::from_page(config.get_module_list_address());
        let mmap = MemoryMap::from_page(config.get_memory_map_address());

        // NOTE: implementing a custom copy trait might allow this to be moved into the `new` call
        // but we don't really want to allow a 'copy' per-se, as there should only ever be one pager, 
        // but in order to return the pager, it must be moved from the stack to the caller's 
        // stack which implies a copy and drop, which can't be done if the page stack's contain 
        // these references...
        self.stack_2mb.set_borrow_source(&self.stack_1gb);
        self.stack_4kb.set_borrow_source(&self.stack_2mb);

        let num_regions = mmap.get_num_regions();
        let kernel_load_info = module_list.get_module_info(0).unwrap();
        let kernel_physical_start = kernel_load_info.get_start_address();
        let kernel_size = kernel_load_info.get_size();
        let required_base = kernel_physical_start + kernel_size as Address;

        // This page allocator is provided to `create_page_stack` as a source for 4kb pages whenever to 
        // pager needs to create a new page table.  
        // Later on, when we create the 4kb page stack, we'll exclude the pages which were returned 
        // from this stack.
        // Note that we also tell the iterator to pick pages from after the kernel as the act of allocating 
        // it has already broken up some 2mb and a 1gb page, so we prefer to not break up any more.
        // NOTE: no provision has been made to ensure that the page stack actually has enough 4kb pages 
        // for this task... at some point I'll need to allow for 2mb pages to be consumed for this 
        // purpose iff 4kb pages run out.
        let mut page_table_allocator = PageIterator::new(&mmap, PAGE_SIZE_4KB)
                .with_region_type(MemoryRegionType::Available)
                .with_base_address(required_base);

        let mut page_allocator = || {
            page_table_allocator.next()
        };

        // TODO: break this up into steps to map the pages in to create the stacks, 
        // and then to push the pages to the stacks, as the former will consume pages which we don't want to push onto the stacks,
        // so we want to know which ones have been consumed before populating the 4kb stack (which is where the page table allocator 
        // is getting pages from) and we can add an exclusion range to that page iterator
        println!("Creating 1GB page stack");
        self.create_page_stack(
            &mut self.stack_1gb.stacks.lock().available_pages, 
            PageIterator::new(&mmap, PAGE_SIZE_1GB)
                .with_region_type(MemoryRegionType::Available), 
            &mut page_allocator);
        println!("Creating 2MB page stack");
        self.create_page_stack(
            &mut self.stack_2mb.stacks.lock().available_pages, 
            PageIterator::new(&mmap, PAGE_SIZE_2MB)
                .with_region_type(MemoryRegionType::Available), 
            &mut page_allocator);
        println!("Creating 4KB page stack");
        self.create_page_stack(
            &mut self.stack_4kb.stacks.lock().available_pages, 
            PageIterator::new(&mmap, PAGE_SIZE_4KB)
                .with_region_type(MemoryRegionType::Available), 
            &mut page_allocator);

        self.populate_page_stack(
            &mut self.stack_1gb.stacks.lock().available_pages, 
            PageIterator::new(&mmap, PAGE_SIZE_1GB)
                .with_region_type(MemoryRegionType::Available));
        self.populate_page_stack(
            &mut self.stack_2mb.stacks.lock().available_pages, 
            PageIterator::new(&mmap, PAGE_SIZE_2MB)
                .with_region_type(MemoryRegionType::Available));
        // The act of creating the page stacks will have consumed some of the available 4kb pages, so we need to
        // exclude those from the page iterator we use to populate the 4kb stack, otherwise we'll end up pushing 
        // pages onto the stack which could be in ues as page tables/dircetories, or mapped in to the page stack itself.
        self.populate_page_stack(
            &mut self.stack_4kb.stacks.lock().available_pages, 
            PageIterator::new(&mmap, PAGE_SIZE_4KB)
                .with_region_type(MemoryRegionType::Available)
                .excluding_range(
                    required_base, 
                    page_table_allocator.get_current().unwrap_or(required_base)));

        // now that all of this in place, we can explicitly allocate the allocated pages 
        // the the page stack itself will ensure the structure is properly mapped 
        // underneath by getting pages from the 4kb stack, and potentially borrowing 
        // from the 2mb or 1gb stacks.

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
            let (pl4_frame, _flags) = Cr3::read();
            let pl4_table = & *(pl4_frame.start_address().as_u64() as *const PageTable);

            let pl4_entry = &pl4_table[pl4_index];
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

    pub fn map_physical_to_virtual(
        &self, 
        phys_addr: PhysicalAddress, 
        virtual_addr: VirtualAddress, 
        flags: x86_64::structures::paging::PageTableFlags) -> Result<(), &'static str> {
        
        self._map_physical_to_virtual( phys_addr, virtual_addr, flags, 
            || { 
                self.stack_4kb.allocate_page().map(
                    |addr| {
                        unsafe {  
                            core::ptr::write_bytes(addr as *mut u8, 0, 0x1000);
                            PhysFrame::<Size4KiB>::from_start_address_unchecked(PhysAddr::new(addr))
                        }
                    }
                )  
            })
    }

    fn _map_physical_to_virtual<F>(
        &self, 
        phys_addr: PhysicalAddress, 
        virtual_addr: VirtualAddress, 
        flags: x86_64::structures::paging::PageTableFlags,
        mut create_page_table: F) -> Result<(), &'static str>

        where F: FnMut() -> Option< PhysFrame::<Size4KiB> > {

        let virtual_addr = virtual_addr.0;
        let phys_addr = phys_addr.0;
        let pl4_index = ((virtual_addr >> 39) & 0o777) as usize;
        let pl3_index = ((virtual_addr >> 30) & 0o777) as usize;
        let pl2_index = ((virtual_addr >> 21) & 0o777) as usize;
        let pl1_index = ((virtual_addr >> 12) & 0o777) as usize;

        println!("Mapping physical address {:x} to virtual address {:x} with flags {:?}", phys_addr, virtual_addr, flags);

        unsafe {
            let (pl4_frame, _flags) = Cr3::read();
            let pl4_table = &mut *(pl4_frame.start_address().as_u64() as *mut PageTable);

            let pl4_entry = &mut pl4_table[pl4_index];
            if pl4_entry.is_unused() {
                let new_frame = (create_page_table)().ok_or("Couldn't create page table")?;
                //info!("Setting PML4 entry {} to new frame at {:?}", pl4_index, new_frame.start_address());
                pl4_entry.set_addr(new_frame.start_address(), flags | x86_64::structures::paging::PageTableFlags::PRESENT);
                //info!("Set PML4 entry {} to new frame at {:?}", pl4_index, new_frame.start_address());
            }

            let pl3_table = &mut *(pl4_entry.addr().as_u64() as *mut PageTable);
            let pl3_entry = &mut pl3_table[pl3_index];
            if pl3_entry.is_unused() {
                let new_frame = (create_page_table)().ok_or("Couldn't create page table")?;
                pl3_entry.set_addr(new_frame.start_address(), flags | x86_64::structures::paging::PageTableFlags::PRESENT);
            }

            let pl2_table = &mut *(pl3_entry.addr().as_u64() as *mut PageTable);
            let pl2_entry = &mut pl2_table[pl2_index];
            if pl2_entry.is_unused() {
                let new_frame = (create_page_table)().ok_or("Couldn't create page table")?;
                pl2_entry.set_addr(new_frame.start_address(), flags | x86_64::structures::paging::PageTableFlags::PRESENT);
            }

            let pl1_table = &mut *(pl2_entry.addr().as_u64() as *mut PageTable);
            let pl1_entry = &mut pl1_table[pl1_index];
            if !pl1_entry.is_unused() {
                return Err("Virtual address already mapped");
            }

            // For simplicity, we only support mapping a single page here. In a real implementation, you'd want to handle larger mappings.
            pl1_entry.set_addr(PhysAddr::new(phys_addr as u64), flags | x86_64::structures::paging::PageTableFlags::PRESENT);
        }

        Ok(())
    }

    pub fn output_mmap(&self) {
        unsafe {
            let (pl4_frame, _flags) = Cr3::read();
            let pl4_table = & *(pl4_frame.start_address().as_u64() as *const PageTable);

            for (i, entry) in pl4_table.iter().enumerate() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use satus_struct::memory_map::MemoryRegionType;

    fn pages_4kb(n: usize) -> Address { (PAGE_SIZE_4KB * n) as Address }
    fn pages_2mb(n: usize) -> Address { (PAGE_SIZE_2MB * n) as Address }
    fn pages_1gb(n: usize) -> Address { (PAGE_SIZE_1GB * n) as Address }

    #[test]
    fn test_page_iterator() {
        // create a mmap with various edge cases
        let mmapPage = [0u8; 4096];
        let mmapPageAddr = mmapPage.as_ptr() as Address;
        let mut mmap = MemoryMap::new_from_page(mmapPageAddr).unwrap();

        let mut base: Address = 0;

        // 1. multiple 4kb pages in a region
        mmap.add_region(MemoryRegionType::Available, base, base + pages_4kb(2));

        // 2. a single 4kb page in a region
        base = pages_4kb(4);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_4kb(1));

        // 3. multiple 2mb pages in a region
        base = PAGE_SIZE_2MB as Address;
        mmap.add_region(MemoryRegionType::Available, base, base + pages_2mb(2));

        // 4. a single 2mb page in a region
        base = pages_2mb(4);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_2mb(1));

        // 5. multiple 1gb pages in a region
        base = PAGE_SIZE_1GB as Address;
        mmap.add_region(MemoryRegionType::Available, base, base + pages_1gb(3));

        // 6. a single 1gb page in a region
        base = pages_1gb(4);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_1gb(1));

        // 7. some 4kb pages followed by a 2mb page
        base = pages_1gb(6) - pages_4kb(2);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_4kb(2) + pages_2mb(1));

        // 8. some 4kb pages followed by a 1gb page
        base = pages_1gb(7) - pages_4kb(2);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_4kb(2) + pages_1gb(1));

        // 9. some 2mb pages followed by a 1gb page
        base = pages_1gb(8) - pages_2mb(2);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_2mb(2) + pages_1gb(1));

        // 10. a 2mb page followed by some 4kb pages
        base = pages_1gb(9) - pages_2mb(1);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_2mb(1) + pages_4kb(2));

        // 11. a 1gb page followed by some 4kb pages
        base = pages_1gb(10);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_1gb(1) + pages_4kb(2));

        // 12. a 1gb page followed by some 2mb pages
        base = pages_1gb(11);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_1gb(1) + pages_2mb(2));

        // 13. some 4kb pages followed by a 2mb page, a 1gb page, then a 2mb page and a 4kb page
        base = pages_1gb(13) - pages_2mb(1) - pages_4kb(2);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_4kb(2) + pages_2mb(1) + pages_1gb(1) + pages_2mb(1) + pages_4kb(1));

        let mut page_iter_4kb = PageIterator::new(&mmap, PAGE_SIZE_4KB);
        let mut page_iter_2mb = PageIterator::new(&mmap, PAGE_SIZE_2MB);
        let mut page_iter_1gb = PageIterator::new(&mmap, PAGE_SIZE_1GB);

        // 1. multiple 4kb pages in a region
        assert_eq!(page_iter_4kb.next(), Some(0));
        assert_eq!(page_iter_4kb.next(), Some(0x1000));
        
        // 2. a single 4kb page in a region
        assert_eq!(page_iter_4kb.next(), Some(0x4000));

        // 3. multiple 2mb pages in a region
        assert_eq!(page_iter_2mb.next(), Some(0x200000));
        assert_eq!(page_iter_2mb.next(), Some(0x400000));

        // 4. a single 2mb page in a region
        assert_eq!(page_iter_2mb.next(), Some(0x800000));

        // 5. multiple 1gb pages in a region
        assert_eq!(page_iter_1gb.next(), Some(0x40000000));
        assert_eq!(page_iter_1gb.next(), Some(0x80000000));
        assert_eq!(page_iter_1gb.next(), Some(0xC0000000));

        // 6. a single 1gb page in a region
        assert_eq!(page_iter_1gb.next(), Some(0x100000000));

        // 7. some 4kb pages followed by a 2mb page
        assert_eq!(page_iter_4kb.next(), Some(0x180000000 - pages_4kb(2)));
        assert_eq!(page_iter_4kb.next(), Some(0x180000000 - pages_4kb(1)));
        assert_eq!(page_iter_2mb.next(), Some(0x180000000));

        // 8. some 4kb pages followed by a 1gb page
        assert_eq!(page_iter_4kb.next(), Some(0x1C0000000 - pages_4kb(2)));
        assert_eq!(page_iter_4kb.next(), Some(0x1C0000000 - pages_4kb(1)));
        assert_eq!(page_iter_1gb.next(), Some(0x1C0000000));

        // 9. some 2mb pages followed by a 1gb page
        assert_eq!(page_iter_2mb.next(), Some(0x200000000 - pages_2mb(2)));
        assert_eq!(page_iter_2mb.next(), Some(0x200000000 - pages_2mb(1)));
        assert_eq!(page_iter_1gb.next(), Some(0x200000000));
                                            
        // 10. a 2mb page followed by some 4kb pages
        assert_eq!(page_iter_2mb.next(), Some(0x240000000 - pages_2mb(1)));
        assert_eq!(page_iter_4kb.next(), Some(0x240000000));
        assert_eq!(page_iter_4kb.next(), Some(0x240000000 + pages_4kb(1)));

        // 11. a 1gb page followed by some 4kb pages
        assert_eq!(page_iter_1gb.next(), Some(0x280000000));
        assert_eq!(page_iter_4kb.next(), Some(0x2C0000000));
        assert_eq!(page_iter_4kb.next(), Some(0x2C0000000 + pages_4kb(1)));

        // 12. a 1gb page followed by some 2mb pages
        assert_eq!(page_iter_1gb.next(), Some(0x2C0000000));
        assert_eq!(page_iter_2mb.next(), Some(0x300000000 + pages_2mb(0)));
        assert_eq!(page_iter_2mb.next(), Some(0x300000000 + pages_2mb(1)));

        // 13. some 4kb pages followed by a 2mb page, a 1gb page, then a 2mb page and a 4kb page
        assert_eq!(page_iter_4kb.next(), Some(0x340000000 - pages_2mb(1) - pages_4kb(2)));
        assert_eq!(page_iter_4kb.next(), Some(0x340000000 - pages_2mb(1) - pages_4kb(1)));
        assert_eq!(page_iter_2mb.next(), Some(0x340000000 - pages_2mb(1)));
        assert_eq!(page_iter_1gb.next(), Some(0x340000000));
        assert_eq!(page_iter_2mb.next(), Some(0x380000000));
        assert_eq!(page_iter_4kb.next(), Some(0x380000000 + pages_2mb(1) + pages_4kb(0)));

        assert_eq!(page_iter_4kb.next(), None);
        assert_eq!(page_iter_2mb.next(), None);
        assert_eq!(page_iter_1gb.next(), None);
    }
}
