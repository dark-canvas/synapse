//! Pager implementation
//!
//! The kernel is mapped to 0xFFFFFF8000000000 (last element in pl4 table)
//!   p4 index = 511
//!   p3 index = 0
//!   p2 index = 0
//!   p1 index = 0
//! The stack base is also at 0xFFFFFF8000000000, but expands downwards
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

use spin::Mutex;

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
use self::page_iterator::PageIterator;

use crate::KERNEL_START;
use crate::KERNEL_STACK_SIZE;

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
    Page4KB,
    Page2MB,
    Page1GB,
}

#[derive(PartialEq, Copy, Clone)]
pub enum SizedPage {
    Page4KB(Address),
    Page2MB(Address),
    Page1GB(Address),
}

pub struct Pager {
    stack_1gb: Mutex< (PageStack::<PAGE_SIZE_1GB>, Address) >,
    stack_2mb: Mutex< (PageStack::<PAGE_SIZE_2MB>, Address) >,
    stack_4kb: Mutex< (PageStack::<PAGE_SIZE_4KB>, Address) >,
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
    fn ensure_mapped(&self, base: VirtualAddress, end: VirtualAddress) -> Result<bool, &'static str> {
        let base = base.0;
        let end = end.0;
        let mut pages_mapped = false;
        let mut current_addr = base;
        while current_addr < end {
            let page_type = if end - current_addr >= PAGE_SIZE_1GB as Address && (current_addr & PAGE_OFFSET_MASK_1GB as Address == 0) {
                PageType::Page1GB
            } else if end - current_addr >= PAGE_SIZE_2MB as Address && (current_addr & PAGE_OFFSET_MASK_2MB as Address == 0) {
                PageType::Page2MB
            } else {
                PageType::Page4KB
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
                PageType::Page1GB => PAGE_SIZE_1GB,
                PageType::Page2MB => PAGE_SIZE_2MB,
                PageType::Page4KB => PAGE_SIZE_4KB,
            }) as Address;
        }

        Ok(pages_mapped)
    }
}

#[allow(dead_code)]
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
            PageType::Page2MB,
            PageTableFlags::WRITABLE, &mut create_page_table).expect("2MB page for 2MB page aggregator");
        // 1gb aggregator == 4kb page
        Self::_map_physical_to_virtual( 
            one_gb_aggregator_base_physical,
            VirtualAddress(PAGE_AGGREGATOR_1GB_BASE), 
            PageType::Page4KB,
            PageTableFlags::WRITABLE, &mut create_page_table).expect("4KB page for 1GB page aggregator");
        // 512gb aggregator == 4kb page
        Self::_map_physical_to_virtual( 
            five_twelve_gb_allocator_base_physical, 
            VirtualAddress(PAGE_AGGREGATOR_512GB_BASE), 
            PageType::Page4KB,
            PageTableFlags::WRITABLE, &mut create_page_table).expect("4KB page for 512GB page aggregator");
    }

    fn map_in_page_stacks_memory(
        mmap: &MemoryMap,
        four_kb_page_allocator: &mut PageIterator) -> (Address, Address, Address) {

        let mut page_allocator = || {
            four_kb_page_allocator.next()
        };

        println!("Creating 1GB page stack");
        let top_of_1gb_stack = Self::map_in_page_stack(
            PAGE_STACK_1GB_BASE, 
            PageIterator::new(&mmap, PAGE_SIZE_1GB)
                .with_region_type(MemoryRegionType::Available), 
            &mut page_allocator);
        println!("Creating 2MB page stack");
        let top_of_2mb_stack = Self::map_in_page_stack(
            PAGE_STACK_2MB_BASE, 
            PageIterator::new(&mmap, PAGE_SIZE_2MB)
                .with_region_type(MemoryRegionType::Available), 
            &mut page_allocator);
        println!("Creating 4KB page stack");
        let top_of_4kb_stack = Self::map_in_page_stack(
            PAGE_STACK_4KB_BASE, 
            PageIterator::new(&mmap, PAGE_SIZE_4KB)
                .with_region_type(MemoryRegionType::Available), 
            &mut page_allocator);

        (top_of_4kb_stack, top_of_2mb_stack, top_of_1gb_stack)
    }

    fn map_in_page_stack<F>(stack_base_address: Address, pages: PageIterator, new_page: &mut F) -> Address
        where F : FnMut() -> Option< Address > {

        let pages_count = pages.get_count();
        let required_stack_size_in_pages = pages_required(pages_count * size_of::<Address>());
        println!("Page stack contains {} addresses, consuming {} pages for the stack structure", pages_count, required_stack_size_in_pages);        

        for i in 0..required_stack_size_in_pages {
            println!("Mapping page {}", i);
            Self::_map_physical_to_virtual(
                PhysicalAddress((new_page)().expect("Unable to map page stack")), 
                VirtualAddress(stack_base_address + (i*PAGE_SIZE_4KB) as Address),
                PageType::Page4KB,
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

        stack_base_address + (required_stack_size_in_pages * PAGE_SIZE_4KB) as Address
    }

    fn ensure_writable(virtual_addr: Address) -> bool {
        let pl4_index = (virtual_addr as usize >> 39) & 0o777;
        let pl3_index = (virtual_addr as usize >> 30) & 0o777;
        let pl2_index = (virtual_addr as usize >> 21) & 0o777;
        let pl1_index = (virtual_addr as usize >> 12) & 0o777;

        println!("        ensure writable 0x{:016x}", virtual_addr);

        unsafe {
            let pl4_table = get_pl4_table();

            let pl4_entry = &pl4_table[pl4_index];
            if pl4_entry.is_unused() {
                println!("        not mapped in pl4");
                return false;
            }

            // page directory entry is 4kb page...
            let pl3_table = &mut *(pl4_entry.addr().as_u64() as *mut PageTable);
            let pl3_entry = &pl3_table[pl3_index];
            if pl3_entry.is_unused() {
                println!("        not mapped in pl3");
                return false;
            }

            // if this entry isn't writable, we need to set it to be writable, but we can't do that 
            // unless the page containing this entry is writable...
            if !pl3_entry.flags().contains(PageTableFlags::WRITABLE) {
                println!("        pl3 set writable");
                if !pl3_entry.flags().contains(PageTableFlags::WRITABLE) {
                    Self::ensure_writable(pl4_entry.addr().as_u64());
                }
                pl3_entry.flags().insert(PageTableFlags::WRITABLE);
            }

            if pl3_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
                println!("        huge page; done");
                return true;
            }


            // this could be a 2mb page...
            let pl2_table = &mut *(pl3_entry.addr().as_u64() as *mut PageTable);
            let pl2_entry = &pl2_table[pl2_index];
            if pl2_entry.is_unused() {
                println!("        not mapped in pl2");
                return false;
            }

            if !pl2_entry.flags().contains(PageTableFlags::WRITABLE) {
                println!("        set writable pl2");
                if !pl3_entry.flags().contains(PageTableFlags::WRITABLE) {
                    Self::ensure_writable(pl3_entry.addr().as_u64());
                }
                pl2_entry.flags().insert(PageTableFlags::WRITABLE);
            }

            if pl2_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
                println!("        huge page pl2; done");
                return true;
            }

            let pl1_table = &mut *(pl2_entry.addr().as_u64() as *mut PageTable);
            let pl1_entry = &pl1_table[pl1_index];
            if pl1_entry.is_unused() {
                println!("        not mapped in pl1");
                return false;
            }

            if !pl1_entry.flags().contains(PageTableFlags::WRITABLE) {
                println!("        set writable pl1");
                if !pl2_entry.flags().contains(PageTableFlags::WRITABLE) {
                    Self::ensure_writable(pl2_entry.addr().as_u64());
                }
                pl1_entry.flags().insert(PageTableFlags::WRITABLE);
            }
            return true;
        }
    }

    fn setup_physical_offset_identity_map() {
        // Physical memory (due to the UEFI firmware and bootloader) is currently identity mapped.
        // We've artificially limited the kernel's support for 512MB of memory, which means the entire contents of physical, 
        // identity mapped, memory is at pl4_table[0].
        // We want to keep this (we want the ability to read/write directly to physical memory in order to create new, or edit 
        // existing, page tables), but we'll want to remap that region of memory with every new proceess so we'll copy 
        // it up higher in the table (at index ...) which means we can then access physical memory by reading/writing to 
        // the physical address + PHYSICAL_OFFSET (0xFFFFFF0000000000)
        let mut pl4_table = get_pl4_table();

        let pl4_first_entry = pl4_table[0].addr();
        pl4_table[510].set_addr(
            pl4_first_entry, 
            PageTableFlags::GLOBAL | PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE );
        // TODO: we need to go through and ensure all the other page tables which the UEFI firmware created are marked as 
        // GLOBAL and WRITABLE
        // We can then unmap everything in the original identity map

        unsafe {
            println!("Iterating...");
            let physical_map_table_entry = &mut pl4_table[510];
            println!("Entry {:?}", physical_map_table_entry);
            let pl3_table = &mut *(physical_map_table_entry.addr().as_u64() as *mut PageTable);
            println!("Table {:?}", pl3_table);

            for (_i, pl3_entry) in pl3_table.iter_mut().enumerate() {
                if !pl3_entry.is_unused() {
                    println!("  pl3 entry {}: {:?}", _i, pl3_entry);

                    let mut flags = pl3_entry.flags();
                    if flags & PageTableFlags::WRITABLE != PageTableFlags::WRITABLE {
                        println!("    setting writable bit");
                        Self::ensure_writable(physical_map_table_entry.addr().as_u64());
                        flags |= /*PageTableFlags::GLOBAL |*/ PageTableFlags::WRITABLE /*| PageTableFlags::NO_EXECUTE */;
                        pl3_entry.set_flags(flags);
                    }

                    if flags & PageTableFlags::HUGE_PAGE == PageTableFlags::HUGE_PAGE {
                        println!("    huge page");
                        continue;
                    }

                    let pl2_table = &mut *(pl3_entry.addr().as_u64() as *mut PageTable);
                    for (_j, pl2_entry) in pl2_table.iter_mut().enumerate() {
                        if !pl2_entry.is_unused() {
                            println!("    pl2 entry {}: {:?}", _j, pl2_entry);

                            let mut flags = pl2_entry.flags();
                            if flags & PageTableFlags::WRITABLE != PageTableFlags::WRITABLE {
                                println!("      setting writable bit");
                                Self::ensure_writable(pl3_entry.addr().as_u64());
                                flags |= PageTableFlags::GLOBAL | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
                                pl2_entry.set_flags(flags);
                            }

                            if flags & PageTableFlags::HUGE_PAGE == PageTableFlags::HUGE_PAGE {
                                println!("      huge page");
                                continue;
                            }
                            //info!("  Page Table Entry {}: {:?}", j, entry);

                            let pl1_table = &mut *(pl2_entry.addr().as_u64() as *mut PageTable);
                            for (_k, pl1_entry) in pl1_table.iter_mut().enumerate() {
                                if !pl1_entry.is_unused() {
                                    println!("      pl1 entry {}: {:?}", _k, pl1_entry);

                                    let mut flags = pl1_entry.flags();
                                    if flags & PageTableFlags::WRITABLE != PageTableFlags::WRITABLE {
                                        println!("        setting writable bit");
                                        Self::ensure_writable(pl2_entry.addr().as_u64());
                                        flags |= PageTableFlags::GLOBAL | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
                                        pl1_entry.set_flags(flags);
                                    }
                                    //info!("    Page Table 2 Entry {}: {:?}", k, entry);
                                }
                            }
                        }
                    }
                }
            }
            println!("Make everything writable; flushing cr3");
            // invalidate everything by re-loading cr3
            let (pl4_frame, flags) = Cr3::read();
            Cr3::write(pl4_frame, flags);
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
        let (top_of_4kb_stack, top_of_2mb_stack, top_of_1gb_stack) = 
            Self::map_in_page_stacks_memory(&mmap, &mut four_kb_page_allocator);

        // The act of creating the page stacks will have consumed some of the available 4kb pages, so we need to
        // exclude those from the page iterator we use to populate the 4kb stack, otherwise we'll end up pushing 
        // pages onto the stack which could be in ues as page tables/dircetories, or mapped in to the page stack itself.
        let available_1gb_pages = PageIterator::new(&mmap, PAGE_SIZE_1GB)
                .with_region_type(MemoryRegionType::Available);
        
        let available_2mb_pages = PageIterator::new(&mmap, PAGE_SIZE_2MB)
                .with_region_type(MemoryRegionType::Available)
                .with_base_address(
                    two_mb_page_allocator.get_current().unwrap_or(0));
        
        let available_4kb_pages = PageIterator::new(&mmap, PAGE_SIZE_4KB)
                .with_region_type(MemoryRegionType::Available)
                .excluding_range(
                    kernel_physical_start,
                    four_kb_page_allocator.get_current().unwrap_or(required_base));

        unsafe {
            let pl4_table = &mut *(pl4_frame.start_address().as_u64() as *mut PageTable);

            // map pl4 table into itself for easier virtual to physical mappings
            // let pl4_entry = &mut pl4_table[510];
            // pl4_entry.set_addr(pl4_addr, PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE );
            
            //Self::setup_physical_offset_identity_map();

            Pager { 
                stack_1gb: Mutex::new( 
                    (
                        PageStack::<PAGE_SIZE_1GB>::new(PAGE_STACK_1GB_BASE, PAGE_STACK_1GB_MAX_PAGES, PAGE_AGGREGATOR_512GB_BASE,
                            available_1gb_pages),
                        top_of_1gb_stack
                    )
                ),
                stack_2mb: Mutex::new(
                    (
                        PageStack::<PAGE_SIZE_2MB>::new(PAGE_STACK_2MB_BASE, PAGE_STACK_2MB_MAX_PAGES, PAGE_AGGREGATOR_1GB_BASE,
                            available_2mb_pages),
                        top_of_2mb_stack
                    )
                ),
                stack_4kb: Mutex::new( 
                    (
                        PageStack::<PAGE_SIZE_4KB>::new(PAGE_STACK_4KB_BASE, PAGE_STACK_4KB_MAX_PAGES, PAGE_AGGREGATOR_2MB_BASE,
                            available_4kb_pages),
                        top_of_4kb_stack
                    )
                ),
            }
        }
    }

    // TODO: need to determine how to properly account for borrowed pages?
    // For example, if we don't have enough 4kb pages, and we need to borrow from the 2mb stack, 
    // the 2mb stack will allocate us a page, and it will increment the allocated count in its aggregator.
    // If the 4kb pages are the freed and aggregated together, the page can be returned to the 2mb stack, and 
    // the allocated count will be decremented.
    // This is fine.
    // Is there a case where pages are aggregated and given to the larger stack so it decrements an allocated 
    // count which was never incremented for it?
    // Having various UTs for these interactions can help prove/deny this.

    pub fn allocate_1gb_page(&self) -> Option<Address> {
        self.stack_1gb.lock().0.allocate_page()
    }

    pub fn allocate_2mb_page(&self) -> Option<Address> {
        let mut stack_2mb = self.stack_2mb.lock();
        match stack_2mb.0.allocate_page() {
            Some(addr) => Some(addr),
            None => {
                println!("Borrowing 1gb page");
                if let Some(addr) = self.allocate_1gb_page() {
                    // TODO: need to ensure there's enough mapped memory to do this
                    for i in 1..512 {
                        stack_2mb.0.give(addr + (i*PAGE_SIZE_2MB) as Address);
                    }
                    Some(addr)
                } else {
                    None
                }
            }
        }
    }

    pub fn allocate_4kb_page(&self) -> Option<Address> {
        let mut stack_4kb = self.stack_4kb.lock();
        match stack_4kb.0.allocate_page() {
            Some(addr) => Some(addr),
            None => {
                println!("Borrowing 2mb page");
                if let Some(addr) = self.allocate_2mb_page() {
                    // TODO: need to ensure there's enough mapped memory to do this
                    for i in 1..512 {
                        stack_4kb.0.give(addr + (i*PAGE_SIZE_4KB) as Address);
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
            PageType::Page4KB => self.allocate_4kb_page(),
            PageType::Page2MB => self.allocate_2mb_page(),
            PageType::Page1GB => self.allocate_1gb_page(),
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

    pub fn free_1gb_page(&self, address: Address) {
        let mut stack_1gb = self.stack_1gb.lock();

        if stack_1gb.0.top() >= stack_1gb.1 {
            self.map_physical_to_virtual( 
                PhysicalAddress(self.stack_4kb.lock().0.allocate_page().unwrap()), 
                VirtualAddress(stack_1gb.1), 
                PageType::Page4KB, 
                PageTableFlags::GLOBAL | PageTableFlags::WRITABLE);
            stack_1gb.1 += PAGE_SIZE_4KB as Address;
        }

        stack_1gb.0.deallocate_page(address);
    }

    pub fn free_2mb_page(&self, address: Address) {
        let mut stack_2mb = self.stack_2mb.lock();

        if stack_2mb.0.top() >= stack_2mb.1 {
            self.map_physical_to_virtual( 
                PhysicalAddress(self.stack_4kb.lock().0.allocate_page().unwrap()), 
                VirtualAddress(stack_2mb.1), 
                PageType::Page4KB, 
                PageTableFlags::GLOBAL | PageTableFlags::WRITABLE);
            stack_2mb.1 += PAGE_SIZE_4KB as Address;
        }

        if let Some(agg_addr) = stack_2mb.0.deallocate_page(address) {
            // TODO: need to ensure there's enough mapped memory to do this
            // TODO: UT for thie behaviour... should this actually be a call to give()?
            // I don't think this should be a give(), because pages are initially aggregated to their largest size, and 
            // given to that page stack.  So a page can only be aggregated to a larger page if it once was a larger page... 
            // Which means it was already accounted for by the larger stack.
            // we were able to aggregate this page back into a 1gb page, so return it to the 1gb stack
            drop(stack_2mb);
            self.free_1gb_page(agg_addr);
        }
    }

    pub fn free_4kb_page(&self, address: Address) {
        let mut stack_4kb = self.stack_4kb.lock();

        if stack_4kb.0.top() >= stack_4kb.1 {
            Self::_map_physical_to_virtual(
                PhysicalAddress(stack_4kb.0.allocate_page().unwrap()), 
                VirtualAddress(stack_4kb.1), 
                PageType::Page4KB, 
                PageTableFlags::GLOBAL | PageTableFlags::WRITABLE, 
                &mut || {
                    // we're already holding the 4kb page stack lock...
                    stack_4kb.0.allocate_page().map(
                        |addr| {
                            unsafe {  
                                core::ptr::write_bytes(addr as *mut u8, 0, 0x1000);
                                PhysFrame::<Size4KiB>::from_start_address_unchecked(PhysAddr::new(addr))
                            }
                        }
                    )
                }
            );
            stack_4kb.1 += PAGE_SIZE_4KB as Address;
        }

        if let Some(agg_addr) = stack_4kb.0.deallocate_page(address) {
            println!("Aggregated to 2mb 0x{:16x}", agg_addr);
            drop(stack_4kb);
            self.free_2mb_page(agg_addr);
            /*
            // we were able to aggregate this page back into a 2mb page, so return it to the 2mb stack
            if let Some(agg_addr) = self.stack_2mb.lock().deallocate_page(agg_addr) {
                println!("Aggregated to 1gb 0x{:16x}", agg_addr);
                // we were able to aggregate this page back into a 1gb page, so return it to the 1gb stack
                self.stack_1gb.lock().deallocate_page(agg_addr);
            }
                */
        }
    }

    pub fn free_page(&self, page_type: PageType, address: Address) {
        match page_type {
            PageType::Page4KB => self.free_4kb_page(address),
            PageType::Page2MB => self.free_2mb_page(address),
            PageType::Page1GB => self.free_1gb_page(address),
        }
    }

    pub fn virtual_to_physical(&self, virtual_addr: usize) -> Option<usize> {
        let pl4_index = (virtual_addr >> 39) & 0o777;
        let pl3_index = (virtual_addr >> 30) & 0o777;
        let pl2_index = (virtual_addr >> 21) & 0o777;
        let pl1_index = (virtual_addr >> 12) & 0o777;

        unsafe {
            let pl4_table = get_pl4_table();

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
                self.stack_4kb.lock().0.allocate_page().map(
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
            let pl4_table = get_pl4_table();

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
                if page_type == PageType::Page1GB {
                    pl3_entry.set_addr(PhysAddr::new(phys_addr as u64), 
                    flags | x86_64::structures::paging::PageTableFlags::PRESENT | x86_64::structures::paging::PageTableFlags::HUGE_PAGE);
                    return Ok(());
                }
                let new_frame = (create_page_table)().ok_or("Couldn't create page table")?;
                pl3_entry.set_addr(new_frame.start_address(), flags | x86_64::structures::paging::PageTableFlags::PRESENT);
            } else if page_type == PageType::Page1GB {
                return Err("Virtual address already mapped");
            }

            let pl2_table = &mut *(pl3_entry.addr().as_u64() as *mut PageTable);
            let pl2_entry = &mut pl2_table[pl2_index];
            if pl2_entry.is_unused() {
                if page_type == PageType::Page2MB {
                    pl2_entry.set_addr(PhysAddr::new(phys_addr as u64), 
                    flags | x86_64::structures::paging::PageTableFlags::PRESENT | x86_64::structures::paging::PageTableFlags::HUGE_PAGE);
                    return Ok(());
                }
                let new_frame = (create_page_table)().ok_or("Couldn't create page table")?;
                pl2_entry.set_addr(new_frame.start_address(), flags | x86_64::structures::paging::PageTableFlags::PRESENT);
            } else if page_type == PageType::Page2MB {
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
                if page_type == PageType::Page1GB {
                    pl3_entry.set_addr(PhysAddr::new(self.allocate_1gb_page().ok_or("Couldn't allocate 1GB page")? as u64), 
                    flags | x86_64::structures::paging::PageTableFlags::PRESENT | x86_64::structures::paging::PageTableFlags::HUGE_PAGE);
                    return Ok(true);
                }
                let new_frame = self.allocate_page_table().ok_or("Couldn't create page table")?;
                pl3_entry.set_addr(new_frame.start_address(), flags | x86_64::structures::paging::PageTableFlags::PRESENT);
            } else if page_type == PageType::Page1GB {
                if pl3_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
                    return Ok(false)
                } else {
                    return Err("Virtual address already mapped with a smaller page size");
                }
            }

            let pl2_table = &mut *(pl3_entry.addr().as_u64() as *mut PageTable);
            let pl2_entry = &mut pl2_table[pl2_index];
            if pl2_entry.is_unused() {
                if page_type == PageType::Page2MB {
                    pl2_entry.set_addr(PhysAddr::new(self.allocate_2mb_page().ok_or("Couldn't allocate 2MB page")? as u64), 
                    flags | x86_64::structures::paging::PageTableFlags::PRESENT | x86_64::structures::paging::PageTableFlags::HUGE_PAGE);
                    return Ok(true);
                }
                let new_frame = self.allocate_page_table().ok_or("Couldn't create page table")?;
                pl2_entry.set_addr(new_frame.start_address(), flags | x86_64::structures::paging::PageTableFlags::PRESENT);
            } else if page_type == PageType::Page2MB {
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
            let pl4_table = get_pl4_table();

            for (_i, entry) in pl4_table.iter().enumerate() {
                if !entry.is_unused() {
                    //info!("pl4 Entry {}: {:?}", i, entry);

                    let page_table = &mut *(entry.addr().as_u64() as *mut PageTable);
                    for (_j, entry) in page_table.iter().enumerate() {
                        if !entry.is_unused() {
                            //info!("  Page Table Entry {}: {:?}", j, entry);

                            let page_table_2 = &mut *(entry.addr().as_u64() as *mut PageTable);
                            for (_k, entry) in page_table_2.iter().enumerate() {
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

pub fn run_time_tests(pager: &Pager) {
    // First test that the physical "itentity mapped" region exists at PHYSICAL_OFFSET
    let page = pager.allocate_page(PageType::Page4KB).expect("Must be able to acquire 4kb page");
    unsafe { core::ptr::write_bytes(page as *mut u8, 0xff, 4096); }

    let phys_test_addr = (page + PHYSICAL_OFFSET) as *const u8;
    let phys_test_array: &[u8; 4096] = unsafe {
        &*(phys_test_addr as *const [u8; 4096])
    };
    assert_eq!(phys_test_array[0], 0xff);

    unsafe { core::ptr::write_bytes(page as *mut u8, 0x80, 4096); }
    assert_eq!(phys_test_array[0], 0x80);

    pager.free_page(PageType::Page4KB, page);

    // Next test that we can allocate all available memory as 4kb pages
    // And that once we free all the pages, they're aggregated back into their original 2mb and 1gb 
    // pages where relevant.
    // This will require using the allocated memory as a linked list, so they the pages can be fully 
    // freed once allocated
    let mut first_page: Option<Address> = None;
    let mut last_page: Address = 0;
    let mut count = 0;
    while let Some(page) = pager.allocate_4kb_page() {
        println!("Allocated 0x{:016x}", page);
        count += 1;

        if first_page == None {
            first_page = Some(page);
        } else {
            unsafe {
                // I technically *don't* need to add physical offset here, as the memory is still identity 
                // mapped in place, as well... but dereferencing address 0 will cause a panic... even though 
                // address 0 is actually mapped and valid...
                let last_page_ptr = (last_page + PHYSICAL_OFFSET) as *mut u64;
                *last_page_ptr = page;
            }
        }
        last_page = page;
    }

    println!("Allocated all pages; now freeing");


    // trace out the pages for debug...
    if let Some(mut address) = first_page {
        for _ in 0..count {
            println!("Addr 0x{:016x}", address);
            if address & (4096-1) != 0 {
                breakpoint();
            }
            // extract the next address before we free this page
            let next_page = unsafe {
                let next_page_ptr = (address + PHYSICAL_OFFSET) as *mut u64;
                *next_page_ptr
            };
            address = next_page;
        }
    }

    // Now free them all back...
    if let Some(mut address) = first_page {
        for _ in 0..count {
            println!("Freeing 0x{:016x}", address);
            // extract the next address before we free this page
            let next_page = unsafe {
                let next_page_ptr = (address + PHYSICAL_OFFSET) as *mut u64;
                *next_page_ptr
            };
            pager.free_4kb_page(address);
            address = next_page;
        }
    }
}

fn breakpoint() -> ! {
    println!("Artificial breakpoint");
    loop{}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_borrow() {
        // Create a 1gb page stack with 2 1gb pages available
        // Create a 2mb page stack without any pages available
        // Create a 4kb page stack without any pages available

        // Allocate a 4kb page, which will try to borrow a 2mb page, 
        // which will try to borrow a 1gb page
    }

    #[test]
    fn test_free_borrowed_page() {
        // Allocate a 4kb page, which requires borrowing a 2mb page
        // Free the page which was allocated
        
        // The 4kb stack should recognize that it can aggregate the page back to a 
        // 2mb page, and give it back to the 2mb stack
    }
}