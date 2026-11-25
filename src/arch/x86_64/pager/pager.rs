//! Architecture-specific page allocator.

use core::fmt::Write;
use spin::Mutex;

use x86_64::registers::control::Cr3;
use x86_64::structures::paging::{PageTable, PageTableFlags, PhysFrame, Size4KiB};
use x86_64::PhysAddr;

use satus_struct::config::Config;
use satus_struct::memory_map::{MemoryMap, MemoryRegionType};
use satus_struct::module_list::ModuleList;

use crate::errors::ErrCode;
use crate::logger::{
    FrameBufferLogger, LOG_AGGREGATE_1GB, LOG_AGGREGATE_2MB, LOG_ALLOC_4KB,
    LOG_BORROW_1GB, LOG_BORROW_2MB, LOG_FREE_1GB, LOG_FREE_2MB, LOG_FREE_4KB,
};
use crate::pager::Pager as PagerInterface;
use crate::pager::{PagerError, PhysicalAddress, VirtualAddress};
use crate::types::Address;

use super::{
    entry_to_table, get_pl4_table, pages_required, physical_mirror, PageType,
    PAGE_AGGREGATOR_1GB_BASE, PAGE_AGGREGATOR_2MB_BASE, PAGE_AGGREGATOR_512GB_BASE,
    PAGE_OFFSET_MASK_1GB, PAGE_OFFSET_MASK_2MB, PAGE_SIZE_1GB, PAGE_SIZE_2MB,
    PAGE_SIZE_4KB, PAGE_STACK_1GB_BASE, PAGE_STACK_1GB_MAX_PAGES,
    PAGE_STACK_2MB_BASE, PAGE_STACK_2MB_MAX_PAGES, PAGE_STACK_4KB_BASE,
    PAGE_STACK_4KB_MAX_PAGES, PHYSICAL_OFFSET, PAGER_MAX_SUPPORTED_MEMORY,
};
use super::page_iterator::PageIterator;
use super::page_stack::{PageMapper, PageStack};


pub struct Pager {
    stack_1gb: Mutex< (PageStack::<PAGE_SIZE_1GB>, Address) >,
    stack_2mb: Mutex< (PageStack::<PAGE_SIZE_2MB>, Address) >,
    stack_4kb: Mutex< (PageStack::<PAGE_SIZE_4KB>, Address) >,
    kernel_cr3: u64,
    // TODO: this should be a recursive mutex; allocating a page in different
    // tasks on the same CPU could result in a deadlock if the scheduler *also*
    // allocates a page (it shouldn't.. but...)
    fb_logger: Mutex< FrameBufferLogger >,
}

impl PageMapper for Pager {
    // TODO: REVISIT: there's too many versions of this?  Is this one useful?  It just returns a bool
    // Is it even used?  The high level pager has ensure_mapped(), but it calls allocate_virtual()
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
                        core::ptr::write_bytes(physical_mirror(addr) as *mut u8, 0, 0x1000);
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
                            core::ptr::write_bytes(physical_mirror(addr) as *mut u8, 0, 0x1000);
                            PhysFrame::<Size4KiB>::from_start_address_unchecked(PhysAddr::new(addr))
                        }
                    })
                }
            ).expect("Unable to map");
        }

        stack_base_address + (required_stack_size_in_pages * PAGE_SIZE_4KB) as Address
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

        let (width, height) = config.get_framebuffer_dimensions();
        let fb_logger = FrameBufferLogger::new(
            config.get_framebuffer_address(),
            width as usize,
            height as usize,
            config.get_framebuffer_bytes_per_line() as usize * 4  // TODO: this isn't bytes_per_line!!!??
        );

        println!("fb logger at 0x{:016x} ({}x{}, {} bytes per line)", 
            config.get_framebuffer_address(), width, height, config.get_framebuffer_bytes_per_line());

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
                .with_region_type(MemoryRegionType::Available)
                .map(PhysicalAddress);
        
        let available_2mb_pages = PageIterator::new(&mmap, PAGE_SIZE_2MB)
                .with_region_type(MemoryRegionType::Available)
                .with_base_address(
                    two_mb_page_allocator.get_current().unwrap_or(0))
                .map(PhysicalAddress);
        
        let available_4kb_pages = PageIterator::new(&mmap, PAGE_SIZE_4KB)
                .with_region_type(MemoryRegionType::Available)
                .excluding_range(
                    kernel_physical_start,
                    four_kb_page_allocator.get_current().unwrap_or(required_base))
                .map(PhysicalAddress);

        let (addr, flags) = Cr3::read_raw();
        let cr3_value = addr.start_address().as_u64() | flags as u64;

        unsafe {
            let pl4_table = &mut *(pl4_frame.start_address().as_u64() as *mut PageTable);
            
            // clear out the UEFI firmware's identity mapping of physical memory as we have our own starting 
            // at PHYSICAL_OFFSET, and the construction of the page stacks has made all of the 
            // [BOOT|RUNTIME]_SERVICES_[CODE|DATA] sections available, so we can't use them anymore
            pl4_table[0].set_unused();

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
                kernel_cr3: cr3_value,
                fb_logger: Mutex::new(fb_logger),
            }
        }
    }

    pub fn get_kernel_cr3(&self) -> u64 {
        self.kernel_cr3
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

    pub fn allocate_1gb_page(&self) -> Option<PhysicalAddress> {
        self.stack_1gb.lock().0.allocate_page()
    }

    pub fn allocate_2mb_page(&self) -> Option<PhysicalAddress> {
        let mut stack_2mb = self.stack_2mb.lock();
        match stack_2mb.0.allocate_page() {
            Some(addr) => Some(addr),
            None => {
                println!("Borrowing 1gb page");
                write!(self.fb_logger.lock(), "{}", LOG_BORROW_1GB).unwrap();
                if let Some(addr) = self.take_1gb_page() {
                    for i in 1..512 {
                        stack_2mb.0.give(addr + i * PAGE_SIZE_2MB);
                    }
                    stack_2mb.0.mark_allocated(addr);
                    Some(addr)
                } else {
                    None
                }
            }
        }
    }

    pub fn allocate_4kb_page(&self) -> Option<PhysicalAddress> {
        write!(self.fb_logger.lock(), "{}", LOG_ALLOC_4KB).unwrap();
        let mut stack_4kb = self.stack_4kb.lock();
        match stack_4kb.0.allocate_page() {
            Some(addr) => Some(addr),
            None => {
                println!("Borrowing 2mb page");
                write!(self.fb_logger.lock(), "{}", LOG_BORROW_2MB).unwrap();
                if let Some(addr) = self.take_2mb_page() {
                    for i in 1..512 {
                        stack_4kb.0.give(addr + i * PAGE_SIZE_4KB);
                    }
                    stack_4kb.0.mark_allocated(addr);
                    Some(addr)
                } else {
                    None
                }
            }
        }
    }

    pub fn allocate_page(&self, page_type: PageType) -> Option<PhysicalAddress> {
        match page_type {
            PageType::Page4KB => self.allocate_4kb_page(),
            PageType::Page2MB => self.allocate_2mb_page(),
            PageType::Page1GB => self.allocate_1gb_page(),
        }
    }

    fn allocate_page_table(&self) -> Option<PhysFrame::<Size4KiB>> {
        self.allocate_4kb_page().map(|addr| {
            unsafe {  
                core::ptr::write_bytes(physical_mirror(addr.0) as *mut u8, 0, 0x1000);
                PhysFrame::<Size4KiB>::from_start_address_unchecked(PhysAddr::new(addr.0))
            }
        })
    }

    fn give_1gb_page(&self, address: PhysicalAddress) {
        let mut stack_1gb = self.stack_1gb.lock();
        stack_1gb.0.give(address);
    }

    fn give_2mb_page(&self, address: PhysicalAddress) {
        let mut stack_2mb = self.stack_2mb.lock();
        if let Some(agg_addr) = stack_2mb.0.give(address) {
            self.give_1gb_page(agg_addr);
        }
    }

    fn take_1gb_page(&self) -> Option<PhysicalAddress> {
        self.stack_1gb.lock().0.take()
    }

    fn take_2mb_page(&self) -> Option<PhysicalAddress> {
        let mut stack_2mb = self.stack_2mb.lock();
        match stack_2mb.0.take() {
            Some(addr) => Some(addr),
            None => {
                write!(self.fb_logger.lock(), "{}", LOG_BORROW_1GB).unwrap();
                println!("Taking 1gb page");
                if let Some(addr) = self.take_1gb_page() {
                    for i in 1..512 {
                    stack_2mb.0.give(addr + i * PAGE_SIZE_2MB);
                    }
                    Some(addr)
                } else {
                    None
                }
            }
        }
    }

    pub fn free_1gb_page(&self, address: PhysicalAddress) {
        let mut stack_1gb = self.stack_1gb.lock();

        if stack_1gb.0.top() >= stack_1gb.1 {
            let _ = self.map_physical_to_virtual(
                self.stack_4kb.lock().0.allocate_page().unwrap(),
                VirtualAddress(stack_1gb.1),
                PageType::Page4KB,
                PageTableFlags::GLOBAL | PageTableFlags::WRITABLE,
            );
            stack_1gb.1 += PAGE_SIZE_4KB as Address;
        }

        write!(self.fb_logger.lock(), "{}", LOG_FREE_1GB).unwrap();
        stack_1gb.0.deallocate_page(address);
    }

    pub fn free_2mb_page(&self, address: PhysicalAddress) {
        let mut stack_2mb = self.stack_2mb.lock();

        if stack_2mb.0.top() >= stack_2mb.1 {
            let _ = self.map_physical_to_virtual(
                self.stack_4kb.lock().0.allocate_page().unwrap(),
                VirtualAddress(stack_2mb.1),
                PageType::Page4KB,
                PageTableFlags::GLOBAL | PageTableFlags::WRITABLE,
            );
            stack_2mb.1 += PAGE_SIZE_4KB as Address;
        }

        write!(self.fb_logger.lock(), "{}", LOG_FREE_2MB).unwrap();
        if let Some(agg_addr) = stack_2mb.0.deallocate_page(address) {
            write!(self.fb_logger.lock(), "{}", LOG_AGGREGATE_1GB).unwrap();
            // TODO: need to ensure there's enough mapped memory to do this
            // TODO: UT for thie behaviour... should this actually be a call to give()?
            // I don't think this should be a give(), because pages are initially aggregated to their largest size, and 
            // given to that page stack.  So a page can only be aggregated to a larger page if it once was a larger page... 
            // Which means it was already accounted for by the larger stack.
            // we were able to aggregate this page back into a 1gb page, so return it to the 1gb stack
            drop(stack_2mb);
            self.give_1gb_page(agg_addr);
        }
    }

    pub fn free_4kb_page(&self, address: PhysicalAddress) {
        let mut stack_4kb = self.stack_4kb.lock();

        if stack_4kb.0.top() >= stack_4kb.1 {
            // TODO: if we're freeing a 4kb page, and the stack is full, can it just extend the top of the stack?
            let _ = Self::_map_physical_to_virtual(
                stack_4kb.0.allocate_page().unwrap(),
                VirtualAddress(stack_4kb.1),
                PageType::Page4KB,
                PageTableFlags::GLOBAL | PageTableFlags::WRITABLE,
                &mut || {
                    // we're already holding the 4kb page stack lock...
                    stack_4kb.0.allocate_page().map(|addr| {
                        unsafe {
                            core::ptr::write_bytes(physical_mirror(addr.0) as *mut u8, 0, 0x1000);
                            PhysFrame::<Size4KiB>::from_start_address_unchecked(PhysAddr::new(addr.0))
                        }
                    })
                },
            );
            stack_4kb.1 += PAGE_SIZE_4KB as Address;
        }

        write!(self.fb_logger.lock(), "{}", LOG_FREE_4KB).unwrap();
        if let Some(agg_addr) = stack_4kb.0.deallocate_page(address) {
            write!(self.fb_logger.lock(), "{}", LOG_AGGREGATE_2MB).unwrap();
            println!("Aggregated to 2mb 0x{:016x}", agg_addr.0);
            drop(stack_4kb);
            self.give_2mb_page(agg_addr);
        }
    }

    pub fn free_page(&self, page_type: PageType, address: PhysicalAddress) {
        match page_type {
            PageType::Page4KB => self.free_4kb_page(address),
            PageType::Page2MB => self.free_2mb_page(address),
            PageType::Page1GB => self.free_1gb_page(address),
        }
    }

    pub fn virtual_to_physical(&self, virtual_addr: VirtualAddress) -> Option<PhysicalAddress> {
        let pl4_index = (virtual_addr.0 as usize >> 39) & 0o777;
        let pl3_index = (virtual_addr.0 as usize >> 30) & 0o777;
        let pl2_index = (virtual_addr.0 as usize >> 21) & 0o777;
        let pl1_index = (virtual_addr.0 as usize >> 12) & 0o777;

            let pl4_table = get_pl4_table();

            let pl4_entry = &pl4_table[pl4_index];
            if pl4_entry.is_unused() {
                return None;
            }

            // page directory entry is 4kb page...
            let pl3_table = entry_to_table(&pl4_entry);
            let pl3_entry = &pl3_table[pl3_index];
            if pl3_entry.is_unused() {
                return None;
            }

            if pl3_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
                return Some(PhysicalAddress(pl3_entry.addr().as_u64() + (virtual_addr.0 & 0x3FFFFFFF)));
            }

            // this could be a 2mb page...
            let pl2_table = entry_to_table(&pl3_entry);
            let pl2_entry = &pl2_table[pl2_index];
            if pl2_entry.is_unused() {
                return None;
            }

            if pl2_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
                return Some(PhysicalAddress(pl2_entry.addr().as_u64() + (virtual_addr.0 & 0x1FFFFF)));
            }

            let pl1_table = entry_to_table(&pl2_entry);
            let pl1_entry = &pl1_table[pl1_index];
            if pl1_entry.is_unused() {
                return None;
            }

            Some(PhysicalAddress(pl1_entry.addr().as_u64() + (virtual_addr.0 & 0xFFF)))
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
                            core::ptr::write_bytes(physical_mirror(addr.0) as *mut u8, 0, 0x1000);
                            PhysFrame::<Size4KiB>::from_start_address_unchecked(PhysAddr::new(addr.0))
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

        println!("Mapping physical address 0x{:x} to virtual address 0x{:x} with flags 0x{:?}", phys_addr, virtual_addr, flags);

        let pl4_table = get_pl4_table();

        let pl4_entry = &mut pl4_table[pl4_index];
        if pl4_entry.is_unused() {
            let new_frame = (create_page_table)().ok_or("Couldn't create page table")?;
            //info!("Setting PML4 entry {} to new frame at {:?}", pl4_index, new_frame.start_address());
            pl4_entry.set_addr(new_frame.start_address(), flags | x86_64::structures::paging::PageTableFlags::PRESENT);
            //info!("Set PML4 entry {} to new frame at {:?}", pl4_index, new_frame.start_address());
        }

        let pl3_table = entry_to_table(&pl4_entry);
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

        let pl2_table = entry_to_table(&pl3_entry);
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

        let pl1_table = entry_to_table(&pl2_entry);
        let pl1_entry = &mut pl1_table[pl1_index];
        if !pl1_entry.is_unused() {
            return Err("Virtual address already mapped");
        }

        // For simplicity, we only support mapping a single page here. In a real implementation, you'd want to handle larger mappings.
        pl1_entry.set_addr(PhysAddr::new(phys_addr as u64), flags | x86_64::structures::paging::PageTableFlags::PRESENT);

        Ok(())
    }

    fn ensure_mapped_page(&self, virtual_addr: VirtualAddress, page_type: PageType, flags: PageTableFlags) -> Result<bool, &'static str> {
        let pl4_table = get_pl4_table();

        let pl4_index = (virtual_addr.0 as usize >> 39) & 0o777;
        let pl3_index = (virtual_addr.0 as usize >> 30) & 0o777;
        let pl2_index = (virtual_addr.0 as usize >> 21) & 0o777;
        let pl1_index = (virtual_addr.0 as usize >> 12) & 0o777;
        
            let pl4_entry = &mut pl4_table[pl4_index];
            if pl4_entry.is_unused() {
                let new_frame = self.allocate_page_table().ok_or("Couldn't create page table")?;
                pl4_entry.set_addr(new_frame.start_address(), flags | x86_64::structures::paging::PageTableFlags::PRESENT);
            }

            let pl3_table = entry_to_table(&pl4_entry);
            let pl3_entry = &mut pl3_table[pl3_index];
            if pl3_entry.is_unused() {
                if page_type == PageType::Page1GB {
                    pl3_entry.set_addr(PhysAddr::new(self.allocate_1gb_page().ok_or("Couldn't allocate 1GB page")?.0),
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

            let pl2_table = entry_to_table(&pl3_entry);
            let pl2_entry = &mut pl2_table[pl2_index];
            if pl2_entry.is_unused() {
                if page_type == PageType::Page2MB {
                    pl2_entry.set_addr(PhysAddr::new(self.allocate_2mb_page().ok_or("Couldn't allocate 2MB page")?.0),
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

            let pl1_table = entry_to_table(&pl2_entry);
            let pl1_entry = &mut pl1_table[pl1_index];
            if !pl1_entry.is_unused() {
                return Ok(false)
            }

            pl1_entry.set_addr(PhysAddr::new(self.allocate_4kb_page().ok_or("Couldn't allocate 4KB page")?.0), flags | x86_64::structures::paging::PageTableFlags::PRESENT);

            Ok(true)
    }

    fn is_mapped(&self, virtual_addr: VirtualAddress) -> bool {
        let pl4_table = get_pl4_table();

        let pl4_index = (virtual_addr.0 as usize >> 39) & 0o777;
        let pl3_index = (virtual_addr.0 as usize >> 30) & 0o777;
        let pl2_index = (virtual_addr.0 as usize >> 21) & 0o777;
        let pl1_index = (virtual_addr.0 as usize >> 12) & 0o777;
        
        let pl4_entry = &mut pl4_table[pl4_index];
        if pl4_entry.is_unused() {
            return false;
        }

        let pl3_table = entry_to_table(&pl4_entry);
        let pl3_entry = &mut pl3_table[pl3_index];
        if pl3_entry.is_unused() {
            return false;
        }

        let pl2_table = entry_to_table(&pl3_entry);
        let pl2_entry = &mut pl2_table[pl2_index];
        if pl2_entry.is_unused() {
            return false;
        }
        // TODO: handle huge pages... (JJW)
        //if pl2_entry.flags().contains(PageTableFlags::HUGE_PAGE) &&
        //    virtual_addr

        let pl1_table = entry_to_table(&pl2_entry);
        let pl1_entry = &mut pl1_table[pl1_index];
        if pl1_entry.is_unused() {
            return false;
        }

        return true;
    }

    
    pub fn show_address_debug(&self, virtual_addr: VirtualAddress) {
        let pl4_table = get_pl4_table();

        let pl4_index = (virtual_addr.0 as usize >> 39) & 0o777;
        let pl3_index = (virtual_addr.0 as usize >> 30) & 0o777;
        let pl2_index = (virtual_addr.0 as usize >> 21) & 0o777;
        let pl1_index = (virtual_addr.0 as usize >> 12) & 0o777;

        println!("Address {:#x} indices:", virtual_addr.0);
        println!("  {} {} {} {}", pl4_index, pl3_index, pl2_index, pl1_index);
        
            let pl4_entry = &mut pl4_table[pl4_index];
            if pl4_entry.is_unused() {
                println!("  no p4 entry");
                return;
            }
            println!("  pl4 entry: {:?}", pl4_entry);

            let pl3_table = entry_to_table(&pl4_entry);
            let pl3_entry = &mut pl3_table[pl3_index];
            if pl3_entry.is_unused() {
                println!("  No pl3 entry");
            }
            println!("  pl3 entry: {:?}", pl3_entry);
            if pl3_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
                return;
            }

            let pl2_table = entry_to_table(&pl3_entry);
            let pl2_entry = &mut pl2_table[pl2_index];
            if pl2_entry.is_unused() {
                println!("  no pl2 entry");
            }
            println!("  pl2 entry {:?}", pl2_entry);
            if pl2_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
                return;
            }

            let pl1_table = entry_to_table(&pl2_entry);
            let pl1_entry = &mut pl1_table[pl1_index];
            if !pl1_entry.is_unused() {
                println!("  no pl1 entry");
            }

            println!("  pl1 entry {:?}", pl1_entry);
    }
}

impl PagerInterface for Pager {
    fn get_page_size(&self) -> usize {
        4096
    }

    fn get_page_size_log2(&self) -> usize {
        12
    }

    fn allocate_physical(&self) -> Result<PhysicalAddress, ErrCode> { 
        self.allocate_4kb_page().ok_or(ErrCode::OutOfMemory)
    }
    fn free_physical(&self, addr: PhysicalAddress) -> Result<(), ErrCode> { 
        self.free_4kb_page(addr);
        Ok(())
    }

    // TODO: move into page interface as a provided method
    fn allocate_virtual(&self, num_pages: usize, base_addr: VirtualAddress) -> Result<VirtualAddress, ErrCode> { 
        let mut to_addr = base_addr;
        let mut first_mapped : Option<VirtualAddress> = None;
        for _ in 0..num_pages {
            if !self.is_mapped(to_addr) {
                println!("0x{:x} not mapped; mapping", to_addr.0);
                let phys_addr = self.allocate_physical()?;
                PagerInterface::map_physical_to_virtual(self, phys_addr, to_addr)?;
                if first_mapped.is_none() {
                    first_mapped = Some(to_addr);
                }
            }
            to_addr += self.get_page_size();
        }
        if first_mapped.is_none() {
            return Err(ErrCode::Pager(PagerError::VirtualAddressAlreadyMapped(base_addr)));
        }
        // TODO: REVISIT... this should return either a page range, or a start + length
        // REVISUT: could it return a PageRanges value?  possibly too complex
        Ok(first_mapped.unwrap())
    }
    fn free_virtual(&self, _num: usize, _base_addr: VirtualAddress) -> Result<(), ErrCode> { Err(ErrCode::Unimplemented) }
    fn map_physical_to_virtual(&self, phys_addr: PhysicalAddress, virt_addr: VirtualAddress) -> Result<(), ErrCode> { 
        self.map_physical_to_virtual(
            phys_addr, 
            virt_addr, 
            PageType::Page4KB,
            PageTableFlags::WRITABLE).map_err( |_| { ErrCode::Pager(PagerError::UnmappedVirtualAddress(virt_addr)) } ) ;
        Ok(())
    }
    fn get_virtual_address(&self, addr: PhysicalAddress) -> Result<VirtualAddress, ErrCode> { 
        Ok(VirtualAddress(addr.0 + PHYSICAL_OFFSET))
    }
    fn get_physical_address(&self, addr: VirtualAddress) -> Result<PhysicalAddress, ErrCode> {
        self.virtual_to_physical(addr).ok_or(
            ErrCode::Pager(PagerError::UnmappedVirtualAddress(addr))
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spin::Mutex;
    use crate::logger::FrameBufferLogger;
    use crate::arch::x86_64::pager::page_stack::PageStack;
    use std::boxed::Box;
    use std::mem::size_of;
    use std::vec;

    use crate::arch::x86_64::pager::address_aggregator::PageBucket;
    use crate::arch::x86_64::pager::page_stack::ADDRESSES_PER_PAGE;

    struct TestPagerConfig {
        count_4kb_pages: usize,
        count_2mb_pages: usize,
        count_1gb_pages: usize,
        count_2mb_borrows: usize,
        count_1gb_borrows: usize,
    }

    impl TestPagerConfig {
        pub fn new() -> Self {
            TestPagerConfig {
                count_4kb_pages: 0,
                count_2mb_pages: 0,
                count_1gb_pages: 0,
                count_2mb_borrows: 0,
                count_1gb_borrows: 0,
            }
        }

        pub fn with_pages(mut self, count_4kb: usize, count_2mb: usize, count_1gb: usize) -> Self {
            self.count_4kb_pages = count_4kb;
            self.count_2mb_pages = count_2mb;
            self.count_1gb_pages = count_1gb;
            self
        }

        pub fn with_borrows(mut self, count_2mb_borrows: usize, count_1gb_borrows: usize) -> Self {
            self.count_2mb_borrows = count_2mb_borrows;
            self.count_1gb_borrows = count_1gb_borrows;
            self
        }
    }

    struct TestPager {
        pager: Pager,
        stack_1gb: Box< [PhysicalAddress] >,
        stack_2mb: Box< [PhysicalAddress] >,
        stack_4kb: Box< [PhysicalAddress] >,
        aggregator_512gb: Box< [PageBucket] >,
        aggregator_1gb: Box< [PageBucket] >,
        aggregator_2mb: Box< [PageBucket] >,
    }

    impl TestPager {
        pub fn from_config(config: TestPagerConfig) -> Self {
            let mut max_address = (config.count_4kb_pages * PAGE_SIZE_4KB) as Address;
            let first_4kb_address = 0u64;
            let mut first_2mb_address = 0u64;
            let mut first_1gb_address = 0u64;
            if config.count_2mb_pages > 0 {
                // round up the nearest 2mb page
                first_2mb_address = ((max_address + PAGE_SIZE_2MB as Address - 1) / PAGE_SIZE_2MB as Address) * PAGE_SIZE_2MB as Address;
                max_address = first_2mb_address + (config.count_2mb_pages * PAGE_SIZE_2MB) as Address;
            }
            if config.count_1gb_pages > 0 {
                // round up to the nearest 1gb page
                first_1gb_address = ((max_address + PAGE_SIZE_1GB as Address - 1) / PAGE_SIZE_1GB as Address) * PAGE_SIZE_1GB as Address;
                max_address = first_1gb_address + (config.count_1gb_pages * PAGE_SIZE_1GB) as Address;
            }

            let total_4kb_stack_addresses = config.count_4kb_pages + config.count_2mb_borrows * ADDRESSES_PER_PAGE;
            let total_2mb_stack_addresses = config.count_2mb_pages + config.count_1gb_borrows * ADDRESSES_PER_PAGE;
            let total_1gb_stack_addresses = config.count_1gb_pages;

            let stack_4kb_memory = vec![PhysicalAddress(0); total_4kb_stack_addresses].into_boxed_slice();
            let stack_2mb_memory = vec![PhysicalAddress(0); total_2mb_stack_addresses].into_boxed_slice();
            let stack_1gb_memory = vec![PhysicalAddress(0); total_1gb_stack_addresses].into_boxed_slice();

            let stack_4kb_memory_addr = (Box::into_raw(stack_4kb_memory) as *mut PhysicalAddress) as Address;
            let stack_2mb_memory_addr = (Box::into_raw(stack_2mb_memory) as *mut PhysicalAddress) as Address;
            let stack_1gb_memory_addr = (Box::into_raw(stack_1gb_memory) as *mut PhysicalAddress) as Address;
            let stack_4kb_memory_addr_top = stack_4kb_memory_addr + (total_4kb_stack_addresses * size_of::<PhysicalAddress>()) as Address;
            let stack_2mb_memory_addr_top = stack_2mb_memory_addr + (total_2mb_stack_addresses * size_of::<PhysicalAddress>()) as Address;
            let stack_1gb_memory_addr_top = stack_1gb_memory_addr + (total_1gb_stack_addresses * size_of::<PhysicalAddress>()) as Address;

            // allocate memory for page aggregators (TODO: calc the real size)
            let page_aggregator_512gb_memory = vec![PageBucket{allocated:0, available:0}; 4096].into_boxed_slice();
            let page_aggregator_1gb_memory = vec![PageBucket{allocated:0, available:0}; 4096].into_boxed_slice();
            let page_aggregator_2mb_memory = vec![PageBucket{allocated:0, available:0}; 262144].into_boxed_slice();

            let page_aggregator_512gb_memory_addr = (Box::into_raw(page_aggregator_512gb_memory) as *mut Address) as Address;
            let page_aggregator_1gb_memory_addr = (Box::into_raw(page_aggregator_1gb_memory) as *mut Address) as Address;
            let page_aggregator_2mb_memory_addr = (Box::into_raw(page_aggregator_2mb_memory) as *mut Address) as Address;


            // allocate memory for each stack and allocator
            let pager = Pager {
                    stack_1gb: Mutex::new(
                        (
                            PageStack::<PAGE_SIZE_1GB>::new(
                                stack_1gb_memory_addr as Address,
                                PAGE_STACK_1GB_MAX_PAGES,
                                page_aggregator_512gb_memory_addr as Address,
                                (first_1gb_address..(first_1gb_address + (config.count_1gb_pages * PAGE_SIZE_1GB) as Address))
                                    .step_by(PAGE_SIZE_1GB as usize)
                                    .map(PhysicalAddress)),
                            stack_1gb_memory_addr_top
                        )
                    ),
                    stack_2mb: Mutex::new(
                        (
                            PageStack::<PAGE_SIZE_2MB>::new(
                                stack_2mb_memory_addr as Address,
                                PAGE_STACK_2MB_MAX_PAGES,
                                page_aggregator_1gb_memory_addr as Address,
                                (first_2mb_address..(first_2mb_address + (config.count_2mb_pages * PAGE_SIZE_2MB) as Address))
                                    .step_by(PAGE_SIZE_2MB as usize)
                                    .map(PhysicalAddress)),
                            stack_2mb_memory_addr_top
                        )
                    ),
                    stack_4kb: Mutex::new(
                        (
                            PageStack::<PAGE_SIZE_4KB>::new(
                                stack_4kb_memory_addr as Address,
                                PAGE_STACK_4KB_MAX_PAGES,
                                page_aggregator_2mb_memory_addr as Address,
                                (first_4kb_address..(first_4kb_address + (config.count_4kb_pages * PAGE_SIZE_4KB) as Address))
                                    .step_by(PAGE_SIZE_4KB as usize)
                                    .map(PhysicalAddress)),
                            stack_4kb_memory_addr_top
                        )
                    ),
                    kernel_cr3: 0,
                    fb_logger: Mutex::new(
                        FrameBufferLogger::new(0x0 as Address, 800, 600, 3200).disable()
                    ),
            };

            TestPager {
                pager: pager,
                stack_1gb: unsafe { Box::from_raw(core::slice::from_raw_parts_mut(stack_1gb_memory_addr as *mut PhysicalAddress, total_1gb_stack_addresses)) },
                stack_2mb: unsafe { Box::from_raw(core::slice::from_raw_parts_mut(stack_2mb_memory_addr as *mut PhysicalAddress, total_2mb_stack_addresses)) },
                stack_4kb: unsafe { Box::from_raw(core::slice::from_raw_parts_mut(stack_4kb_memory_addr as *mut PhysicalAddress, total_4kb_stack_addresses)) },
                aggregator_512gb: unsafe { Box::from_raw(core::slice::from_raw_parts_mut(page_aggregator_512gb_memory_addr as *mut PageBucket, 4096)) },
                aggregator_1gb: unsafe { Box::from_raw(core::slice::from_raw_parts_mut(page_aggregator_1gb_memory_addr as *mut PageBucket, 4096)) },
                aggregator_2mb: unsafe { Box::from_raw(core::slice::from_raw_parts_mut(page_aggregator_2mb_memory_addr as *mut PageBucket, 262144)) },
            }
        }
    }

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


    #[test]
    fn test_alloc_4kb_simple() {
        let test_pager = TestPager::from_config(
            TestPagerConfig::new()
                .with_pages(3, 1, 1)
        );

        assert_eq!(test_pager.stack_4kb.len(), 3);
        assert_eq!(test_pager.aggregator_2mb[0].available, 3);
        assert_eq!(test_pager.aggregator_2mb[0].allocated, 0);

        assert_eq!(test_pager.pager.allocate_4kb_page(), Some(test_pager.stack_4kb[2]));
        assert_eq!(test_pager.pager.allocate_4kb_page(), Some(test_pager.stack_4kb[1]));
        assert_eq!(test_pager.pager.allocate_4kb_page(), Some(test_pager.stack_4kb[0]));
        assert_eq!(test_pager.aggregator_2mb[0].available, 0);
        assert_eq!(test_pager.aggregator_2mb[0].allocated, 3);
    }

    #[test]
    fn test_alloc_4kb_with_2mb_borrow() {
        let test_pager = TestPager::from_config(
            TestPagerConfig::new()
                .with_pages(2, 1, 1)
                .with_borrows(1, 0)
        );

        assert_eq!(test_pager.aggregator_2mb[0].available, 2);
        assert_eq!(test_pager.aggregator_2mb[0].allocated, 0);
        assert_eq!(test_pager.aggregator_1gb[0].available, 1);
        assert_eq!(test_pager.aggregator_1gb[0].allocated, 0);

        assert_eq!(test_pager.pager.allocate_4kb_page(), Some(test_pager.stack_4kb[1]));
        assert_eq!(test_pager.pager.allocate_4kb_page(), Some(test_pager.stack_4kb[0]));
        assert_eq!(test_pager.aggregator_2mb[0].available, 0);
        assert_eq!(test_pager.aggregator_2mb[0].allocated, 2);
        assert_eq!(test_pager.aggregator_1gb[0].available, 1);
        assert_eq!(test_pager.aggregator_1gb[0].allocated, 0);


        assert_eq!(test_pager.pager.allocate_4kb_page(), Some(test_pager.stack_2mb[0]));
        // allocations from before are unaffected
        assert_eq!(test_pager.aggregator_2mb[0].available, 0);
        assert_eq!(test_pager.aggregator_2mb[0].allocated, 2);
        // borrowed the 2nd full 2mb page, and returned the first 4kb page from it...
        assert_eq!(test_pager.aggregator_2mb[1].available, 511);
        assert_eq!(test_pager.aggregator_2mb[1].allocated, 1);
        // we no longer have this 2mb page anymore as it was provided to the 4kb stack...
        assert_eq!(test_pager.aggregator_1gb[0].available, 0);
        assert_eq!(test_pager.aggregator_1gb[0].allocated, 0);
    }

}
