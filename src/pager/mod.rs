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
//!   - 0xFFFFFFD000000000 - 0xFFFFFFE000000000 -> 4kb page management (prefer to only allocate these for page tables?)
//!      - 0xFFFFFFD000000000 - 0xFFFFFFD040000000 -> 4kb page stack (512GB / 4KB * 8 bytes per entry) == 1gb (allocate multiple 2mb pages for this?)
//!      - 0xFFFFFFD040000000 - 0xFFFFFFD040100000 -> 2mb page aggregator (512GB / 2MB * sizeof(PageBucket) (4)) == 1mb (could allocate a 2mb page for this?)
//!   - 0xFFFFFFE000000000 - 0xFFFFFFE000200000 -> 2mb page management
//!      - 0xFFFFFFE000000000 - 0xFFFFFFE000200000 -> 2mb page stack (512GB / 2MB * 8 bytes per entry) == 2m (allocate a 2mb page for this?)
//!      - 0xFFFFFFE000200000 - 0xFFFFFFE000201000 -> 1gb page aggregator (512GB / 1GB * sizeof(PageBucket) (4)) == 2kb (rounded to 1 page)
//!   - 0xFFFFFFE000201000 - 0xFFFFFFE000202000 -> 1gb page management
//!      - 0xFFFFFFE000201000 - 0xFFFFFFE000202000 -> 1gb page stack (512GB / 1GB * 8 bytes per entry) == 4kb
//!      - 0xFFFFFFE000202000 - 0xFFFFFFE000203000 -> 512gb page aggregator is really just a single PageBucket
//!
//! Note that, for symmetry, I've createad a 512GB page aggregator (eg. 1GB * ADDRESSES_PER_PAGE).
//! There isn't a notion of a 512GB page in the 4 level paging that is used by this kernel, however, 
//! 5 level paging does exist, and does introduce the notion of a 512GB page, so this aggregator allows 
//! for more easily adapting to 5 level paging in the future.
//! However, the current intent of this structure isn't to check whether we can aggregate 512 adjacent 
//! 1GB pages and return them to a larger page stack, but instead just a simple tracking of 
//! available/allocated pages (and it simplifies the code).
//! The simplification breaks if the host platform actually has >= 512GB of memory, as the aggregator 
//! will try to consolodate adjacent pages into a 512GB page if it can, but this is an intentional 
//! design decision/tradeoff -- the current kernel does not support more than 512GB of physical memory.
//! Supporting >= 512GB of physical memory will likely also involve supporting 5 level paging.

pub mod page_stack;
pub mod page_iterator;
pub mod address_aggregator;

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
use self::page_stack::{PageStack, PageMapper};
use self::address_aggregator::AddressAggregator;
use self::page_stack::Stacks;
use self::page_stack::ADDRESSES_PER_PAGE;
use self::page_iterator::PageIterator;

pub const PAGER_MAX_SUPPORTED_MEMORY: usize = 512*1024*1024*1024; // 512GB

pub const PAGE_SIZE_4KB: usize = 4*1024;
pub const PAGE_SIZE_2MB: usize = 2*1024*1024;
pub const PAGE_SIZE_1GB: usize = 1*1024*1024*1024;

pub const PAGE_OFFSET_MASK_4KB: Address = 4*1024-1;
pub const PAGE_OFFSET_MASK_2MB: Address = 2*1024*1024-1;
pub const PAGE_OFFSET_MASK_1GB: Address = 1*1024*1024*1024-1;

pub const PAGE_MASK_4KB: Address = !PAGE_OFFSET_MASK_4KB;
pub const PAGE_MASK_2MB: Address = !PAGE_OFFSET_MASK_2MB;
pub const PAGE_MASK_1GB: Address = !PAGE_OFFSET_MASK_1GB;

const PHYSICAL_OFFSET: Address = 0xFFFFFF0000000000;

const PAGE_STACK_4KB_BASE: Address = 0xFFFFFFD000000000;
const PAGE_STACK_2MB_BASE: Address = 0xFFFFFFE000000000;
const PAGE_STACK_1GB_BASE: Address = 0xFFFFFFE000201000;

const PAGE_STACK_4KB_MAX_PAGES: usize = 134217728;
const PAGE_STACK_2MB_MAX_PAGES: usize = 262144;
const PAGE_STACK_1GB_MAX_PAGES: usize = 512;

const PAGE_AGGREGATOR_2MB_BASE: Address = 0xFFFFFFD040000000;
const PAGE_AGGREGATOR_1GB_BASE: Address = 0xFFFFFFE000200000;
const PAGE_AGGREGATOR_512GB_BASE: Address = 0xFFFFFFE000202000;

#[derive(Copy, Clone)]
pub struct PhysicalAddress(Address);
#[derive(Copy, Clone)]
pub struct VirtualAddress(Address);

#[derive(PartialEq, Copy, Clone)]
pub enum PageType {
    Page4K,
    Page2M,
    Page1G,
}

#[derive(PartialEq, Copy, Clone)]
pub enum SizedPage {
    Page4K(Address),
    Page2M(Address),
    Page1G(Address),
}

type PageAllocator = fn() -> Result< Address, &'static str>;
// Not sure if the error string is actually useful here
type CreatePageTable = fn() -> Result< PhysFrame::<Size4KiB>, &'static str>;

pub struct Pager {
    stack_1gb: PageStack::<PAGE_SIZE_1GB>,
    stack_2mb: PageStack::<PAGE_SIZE_2MB>,
    stack_4kb: PageStack::<PAGE_SIZE_4KB>,
}

pub fn pages_required(size: usize) -> usize {
    (size + (PAGE_SIZE_4KB - 1)) / PAGE_SIZE_4KB
}

pub fn get_pl4_table() -> &'static mut PageTable {
    unsafe {
        let (pl4_frame, _flags) = Cr3::read();
        &mut *(pl4_frame.start_address().as_u64() as *mut PageTable)
    }
}

pub fn is_1gb_aligned(addr: Address) -> bool {
    addr & PAGE_OFFSET_MASK_1GB == 0
}

pub fn is_2mb_aligned(addr: Address) -> bool {
    addr & PAGE_OFFSET_MASK_2MB == 0
}

pub fn is_4kb_aligned(addr: Address) -> bool {
    addr & PAGE_OFFSET_MASK_4KB == 0
}

pub fn next_1gb_page(addr: Address) -> Address {
    (addr + PAGE_SIZE_1GB as Address) & PAGE_MASK_1GB
}

pub fn next_2mb_page(addr: Address) -> Address {
    (addr + PAGE_SIZE_2MB as Address) & PAGE_MASK_2MB
}

pub fn next_4kb_page(addr: Address) -> Address {
    (addr + PAGE_SIZE_4KB as Address) & PAGE_MASK_4KB
}

impl PageMapper for Pager {
    // TODO: specify the page size?
    // TODO: address -> VirtualAddress
    fn ensure_mapped(&self, base: Address, end: Address) -> Result<bool, &'static str> {
        let mut pages_mapped = false;
        let mut current_addr = base;
        while current_addr < end {
            let page_type = if end - current_addr >= PAGE_SIZE_1GB as Address && (current_addr & PAGE_OFFSET_MASK_1GB as Address == 0) {
                PageType::Page1G
            } else if end - current_addr >= PAGE_SIZE_2MB as Address && (current_addr & PAGE_OFFSET_MASK_2MB as Address == 0) {
                PageType::Page2M
            } else {
                PageType::Page4K
            };

            match self.ensure_mapped_page(
                VirtualAddress(current_addr), 
                page_type, 
                PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE) {
                    Ok(mapped) => {
                        if mapped {
                            pages_mapped = true;
                        }
                    },
                    Err(e) => return Err(e),
                }

            current_addr += (match page_type {
                PageType::Page1G => PAGE_SIZE_1GB,
                PageType::Page2M => PAGE_SIZE_2MB,
                PageType::Page4K => PAGE_SIZE_4KB,
            }) as Address;
        }

        Ok(pages_mapped)
    }
}

impl Pager {

    fn map_in_page_aggregator_memory(four_kb_page_allocator: &mut PageIterator, two_mb_page_allocator: &mut PageIterator) {
        let two_mb_aggregator_base_physical = PhysicalAddress(two_mb_page_allocator.next().unwrap()); 
        let one_gb_aggregator_base_physical = PhysicalAddress(four_kb_page_allocator.next().unwrap());
        let five_twelve_gb_allocator_base_physical = PhysicalAddress(four_kb_page_allocator.next().unwrap());    

        let mut create_page_table = || {
             four_kb_page_allocator.next().map(
                |addr| {
                    unsafe {  
                        core::ptr::write_bytes(addr as *mut u8, 0, 0x1000);
                        PhysFrame::<Size4KiB>::from_start_address_unchecked(PhysAddr::new(addr))
                    }
                }
            )  
        };

        // Map in memory for the aggregators
        // 2mb aggregators == 2mb page
        Self::_map_physical_to_virtual( 
            two_mb_aggregator_base_physical, 
            VirtualAddress(PAGE_AGGREGATOR_2MB_BASE), 
            PageType::Page2M,
            PageTableFlags::WRITABLE, &mut create_page_table);
        // 1gb aggregator == 4kb page
        Self::_map_physical_to_virtual( 
            one_gb_aggregator_base_physical,
            VirtualAddress(PAGE_AGGREGATOR_1GB_BASE), 
            PageType::Page4K,
            PageTableFlags::WRITABLE, &mut create_page_table);
        // 512gb aggregator == 4kb page
        Self::_map_physical_to_virtual( 
            five_twelve_gb_allocator_base_physical, 
            VirtualAddress(PAGE_AGGREGATOR_512GB_BASE), 
            PageType::Page4K,
            PageTableFlags::WRITABLE, &mut create_page_table);
    }

    fn map_in_page_stacks_memory(
        mmap: &MemoryMap,
        four_kb_page_allocator: &mut PageIterator) {

        let mut page_allocator = || {
            four_kb_page_allocator.next()
        };

        println!("Creating 1GB page stack");
        Self::map_in_page_stack(
            PAGE_STACK_1GB_BASE, 
            PageIterator::new(&mmap, PAGE_SIZE_1GB)
                .with_region_type(MemoryRegionType::Available), 
            &mut page_allocator);
        println!("Creating 2MB page stack");
        Self::map_in_page_stack(
            PAGE_STACK_2MB_BASE, 
            PageIterator::new(&mmap, PAGE_SIZE_2MB)
                .with_region_type(MemoryRegionType::Available), 
            &mut page_allocator);
        println!("Creating 4KB page stack");
        Self::map_in_page_stack(
            PAGE_STACK_4KB_BASE, 
            PageIterator::new(&mmap, PAGE_SIZE_4KB)
                .with_region_type(MemoryRegionType::Available), 
            &mut page_allocator);
    }

    fn map_in_page_stack<F>(stack_base_address: Address, pages: PageIterator, new_page: &mut F) 
        where F : FnMut() -> Option< Address > {

        let pages_count = pages.get_count();
        let required_stack_size_in_pages = pages_required(pages_count * size_of::<Address>());
        println!("Page stack contains {} addresses, consuming {} pages for the stack structure", pages_count, required_stack_size_in_pages);        

        for i in 0..required_stack_size_in_pages {
            println!("Mapping page {}", i);
            Self::_map_physical_to_virtual(
                PhysicalAddress((new_page)().expect("Unable to map page stack")), 
                VirtualAddress(stack_base_address + (i*PAGE_SIZE_4KB) as Address),
                PageType::Page4K,
                PageTableFlags::WRITABLE,
                &mut || {
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
    pub fn new(config: &Config) -> Self {
        let (pl4_frame, _flags) = Cr3::read();
        let pl4_addr: PhysAddr = pl4_frame.start_address();

        // Ensure the maximum supported memory is not exceeded
        let mmap = MemoryMap::from_page(config.get_memory_map_address());        
        let last_region = mmap.get_memory_region(mmap.get_num_regions() - 1).expect("Memory map must contain at least one region");
        let max_physical_address = last_region.get_end_address();
        assert!( max_physical_address <= PAGER_MAX_SUPPORTED_MEMORY as Address, 
            "Platform has more physical memory ({:#x}) than the maximum supported by this pager ({:#x})", 
            max_physical_address, PAGER_MAX_SUPPORTED_MEMORY);

        //let num_regions = mmap.get_num_regions();
        let module_list = ModuleList::from_page(config.get_module_list_address());
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
        let mut four_kb_page_allocator = PageIterator::new(&mmap, PAGE_SIZE_4KB)
                .with_region_type(MemoryRegionType::Available)
                .with_base_address(required_base);

        // There are a few structures which make sense to allocate using 2mb pages
        // - the 2mb page stack (which is exactly 2mb)
        // - the 2mb page aggregator (which is 1mb total; leaves room for expansion)
        // - the 4kb page stack (for tlb efficiency reasons, it makes sense to *try* to use 2mb pages here)
        let mut two_mb_page_allocator = PageIterator::new(&mmap, PAGE_SIZE_2MB)
            .with_region_type(MemoryRegionType::Available);

        // map in memory for the page aggregators and page stacks
        Self::map_in_page_aggregator_memory(&mut four_kb_page_allocator, &mut two_mb_page_allocator);
        Self::map_in_page_stacks_memory(&mmap, &mut four_kb_page_allocator);

        // The act of creating the page stacks will have consumed some of the available 4kb pages, so we need to
        // exclude those from the page iterator we use to populate the 4kb stack, otherwise we'll end up pushing 
        // pages onto the stack which could be in ues as page tables/dircetories, or mapped in to the page stack itself.
        let mut available_1gb_pages = PageIterator::new(&mmap, PAGE_SIZE_1GB)
                .with_region_type(MemoryRegionType::Available);
        
        let mut available_2mb_pages = PageIterator::new(&mmap, PAGE_SIZE_2MB)
                .with_region_type(MemoryRegionType::Available)
                .with_base_address(
                    two_mb_page_allocator.get_current().unwrap_or(0));
        
        let mut available_4kb_pages = PageIterator::new(&mmap, PAGE_SIZE_4KB)
                .with_region_type(MemoryRegionType::Available)
                .excluding_range(
                    required_base, 
                    four_kb_page_allocator.get_current().unwrap_or(required_base));

        unsafe {
            let pl4_table = &mut *(pl4_frame.start_address().as_u64() as *mut PageTable);

            // map into itself for easier virtual to physical mappings
            let pl4_entry = &mut pl4_table[510];
            pl4_entry.set_addr(pl4_addr, PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE );
            // Or... map all of physical memory, identity mapped into the second last pl4 entry

            Pager { 
                // TODO: rename?  This isn't just a page stack, it's also possibly an aggregator?
                // TODO: borrow available_*_pages?
                stack_1gb: PageStack::<PAGE_SIZE_1GB>::new(PAGE_STACK_1GB_BASE, PAGE_STACK_1GB_MAX_PAGES, PAGE_AGGREGATOR_512GB_BASE,
                    available_1gb_pages),
                stack_2mb: PageStack::<PAGE_SIZE_2MB>::new(PAGE_STACK_2MB_BASE, PAGE_STACK_2MB_MAX_PAGES, PAGE_AGGREGATOR_1GB_BASE,
                    available_2mb_pages),
                stack_4kb: PageStack::<PAGE_SIZE_4KB>::new(PAGE_STACK_4KB_BASE, PAGE_STACK_4KB_MAX_PAGES, PAGE_AGGREGATOR_2MB_BASE,
                    available_4kb_pages),
            }
        }
    }

    pub fn allocate_1gb_page(&self) -> Option<Address> {
        self.stack_1gb.allocate_page(self)
    }

    pub fn allocate_2mb_page(&self) -> Option<Address> {
        match self.stack_2mb.allocate_page(self) {
            Some(addr) => Some(addr),
            None => {
                if let Some(addr) = self.stack_1gb.allocate_page(self) {
                    for i in 1..512 {
                        self.stack_2mb.stacks.lock().available_pages.push(addr + (i*PAGE_SIZE_2MB) as Address);
                    }
                    Some(addr)
                } else {
                    None
                }
            }
        }
    }

    pub fn allocate_4kb_page(&self) -> Option<Address> {
        match self.stack_4kb.allocate_page(self) {
            Some(addr) => Some(addr),
            None => {
                if let Some(addr) = self.stack_2mb.allocate_page(self) {
                    for i in 1..512 {
                        self.stack_4kb.stacks.lock().available_pages.push(addr + (i*PAGE_SIZE_4KB) as Address);
                    }
                    Some(addr)
                } else {
                    None
                }
            }
        }
    }

    pub fn allocate_page(&self, page_type: PageType) -> Option<Address> {
        match page_type {
            PageType::Page4K => self.allocate_4kb_page(),
            PageType::Page2M => self.allocate_2mb_page(),
            PageType::Page1G => self.allocate_1gb_page(),
        }
    }

    fn allocate_page_table(&self) -> Option<PhysFrame::<Size4KiB>> {
        self.allocate_4kb_page().map(|addr| {
            unsafe {  
                core::ptr::write_bytes(addr as *mut u8, 0, 0x1000);
                PhysFrame::<Size4KiB>::from_start_address_unchecked(PhysAddr::new(addr))
            }
        })
    }

    pub fn free_page_1gb(&self, address: Address) {
        self.stack_1gb.deallocate_page(address);
    }

    pub fn free_page_2mb(&self, address: Address) {
        if let Some(agg_addr) = self.stack_2mb.deallocate_page(address) {
            // we were able to aggregate this page back into a 1gb page, so return it to the 1gb stack
            self.stack_1gb.deallocate_page(agg_addr);
        }
    }

    pub fn free_page_4kb(&self, address: Address) {
        if let Some(agg_addr) = self.stack_4kb.deallocate_page(address) {
            // we were able to aggregate this page back into a 2mb page, so return it to the 2mb stack
            if let Some(agg_addr) = self.stack_2mb.deallocate_page(agg_addr) {
                // we were able to aggregate this page back into a 1gb page, so return it to the 1gb stack
                self.stack_1gb.deallocate_page(agg_addr);
            }
        }
    }

    pub fn free_page(&self, page_type: PageType, address: Address) {
        match page_type {
            PageType::Page4K => self.free_page_4kb(address),
            PageType::Page2M => self.free_page_2mb(address),
            PageType::Page1G => self.free_page_1gb(address),
        }
    }

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
            let pl3_table = &mut *(pl4_entry.addr().as_u64() as *mut PageTable);
            let pl3_entry = &pl3_table[pl3_index];
            if pl3_entry.is_unused() {
                return None;
            }

            if pl3_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
                return Some(pl3_entry.addr().as_u64() as usize + (virtual_addr & 0x3FFFFFFF));
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
        page_type: PageType,
        flags: x86_64::structures::paging::PageTableFlags) -> Result<(), &'static str> {
        
        Self::_map_physical_to_virtual( phys_addr, virtual_addr, page_type, flags, 
            &mut || { 
                self.stack_4kb.allocate_page(self).map(
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
        phys_addr: PhysicalAddress, 
        virtual_addr: VirtualAddress, 
        page_type: PageType,
        flags: x86_64::structures::paging::PageTableFlags,
        create_page_table: &mut F) -> Result<(), &'static str>

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
                if page_type == PageType::Page1G {
                    pl3_entry.set_addr(PhysAddr::new(phys_addr as u64), 
                    flags | x86_64::structures::paging::PageTableFlags::PRESENT | x86_64::structures::paging::PageTableFlags::HUGE_PAGE);
                    return Ok(());
                }
                let new_frame = (create_page_table)().ok_or("Couldn't create page table")?;
                pl3_entry.set_addr(new_frame.start_address(), flags | x86_64::structures::paging::PageTableFlags::PRESENT);
            } else if page_type == PageType::Page1G {
                return Err("Virtual address already mapped");
            }

            let pl2_table = &mut *(pl3_entry.addr().as_u64() as *mut PageTable);
            let pl2_entry = &mut pl2_table[pl2_index];
            if pl2_entry.is_unused() {
                if page_type == PageType::Page2M {
                    pl2_entry.set_addr(PhysAddr::new(phys_addr as u64), 
                    flags | x86_64::structures::paging::PageTableFlags::PRESENT | x86_64::structures::paging::PageTableFlags::HUGE_PAGE);
                    return Ok(());
                }
                let new_frame = (create_page_table)().ok_or("Couldn't create page table")?;
                pl2_entry.set_addr(new_frame.start_address(), flags | x86_64::structures::paging::PageTableFlags::PRESENT);
            } else if page_type == PageType::Page2M {
                return Err("Virtual address already mapped");
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

    fn ensure_mapped_page(&self, virtual_addr: VirtualAddress, page_type: PageType, flags: PageTableFlags) -> Result<bool, &'static str> {
        let pl4_table = get_pl4_table();

        let pl4_index = (virtual_addr.0 as usize >> 39) & 0o777;
        let pl3_index = (virtual_addr.0 as usize >> 30) & 0o777;
        let pl2_index = (virtual_addr.0 as usize >> 21) & 0o777;
        let pl1_index = (virtual_addr.0 as usize >> 12) & 0o777;
        
        unsafe {
            let pl4_entry = &mut pl4_table[pl4_index];
            if pl4_entry.is_unused() {
                let new_frame = self.allocate_page_table().ok_or("Couldn't create page table")?;
                pl4_entry.set_addr(new_frame.start_address(), flags | x86_64::structures::paging::PageTableFlags::PRESENT);
            }

            let pl3_table = &mut *(pl4_entry.addr().as_u64() as *mut PageTable);
            let pl3_entry = &mut pl3_table[pl3_index];
            if pl3_entry.is_unused() {
                if page_type == PageType::Page1G {
                    pl3_entry.set_addr(PhysAddr::new(self.allocate_1gb_page().ok_or("Couldn't allocate 1GB page")? as u64), 
                    flags | x86_64::structures::paging::PageTableFlags::PRESENT | x86_64::structures::paging::PageTableFlags::HUGE_PAGE);
                    return Ok(true);
                }
                let new_frame = self.allocate_page_table().ok_or("Couldn't create page table")?;
                pl3_entry.set_addr(new_frame.start_address(), flags | x86_64::structures::paging::PageTableFlags::PRESENT);
            } else if page_type == PageType::Page1G {
                if pl3_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
                    return Ok(false)
                } else {
                    return Err("Virtual address already mapped with a smaller page size");
                }
            }

            let pl2_table = &mut *(pl3_entry.addr().as_u64() as *mut PageTable);
            let pl2_entry = &mut pl2_table[pl2_index];
            if pl2_entry.is_unused() {
                if page_type == PageType::Page2M {
                    pl2_entry.set_addr(PhysAddr::new(self.allocate_2mb_page().ok_or("Couldn't allocate 2MB page")? as u64), 
                    flags | x86_64::structures::paging::PageTableFlags::PRESENT | x86_64::structures::paging::PageTableFlags::HUGE_PAGE);
                    return Ok(true);
                }
                let new_frame = self.allocate_page_table().ok_or("Couldn't create page table")?;
                pl2_entry.set_addr(new_frame.start_address(), flags | x86_64::structures::paging::PageTableFlags::PRESENT);
            } else if page_type == PageType::Page2M {
                if pl2_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
                    return Ok(false)
                } else {
                    return Err("Virtual address already mapped with a smaller page size");
                }
            }

            let pl1_table = &mut *(pl2_entry.addr().as_u64() as *mut PageTable);
            let pl1_entry = &mut pl1_table[pl1_index];
            if !pl1_entry.is_unused() {
                return Ok(false)
            }

            pl1_entry.set_addr(PhysAddr::new(self.allocate_4kb_page().ok_or("Couldn't allocate 4KB page")? as u64), flags | x86_64::structures::paging::PageTableFlags::PRESENT);

            Ok(true)
        }
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
