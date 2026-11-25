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

mod pager;
pub use self::pager::Pager;

use x86_64::registers::control::Cr3;
use x86_64::structures::paging::PageTable;
use x86_64::structures::paging::PageTableFlags;
use x86_64::structures::paging::page_table::PageTableEntry;
use crate::pager::VirtualAddress;
use crate::types::Address;
use super::X86_PAGER;

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

pub(crate) const PHYSICAL_OFFSET: Address = 0xFFFFFF0000000000;

const PAGE_STACK_4KB_BASE: Address = 0xFFFFFFD000000000;
const PAGE_STACK_2MB_BASE: Address = 0xFFFFFFE000000000;
const PAGE_STACK_1GB_BASE: Address = 0xFFFFFFE000201000;

const PAGE_STACK_4KB_MAX_PAGES: usize = 134217728;
const PAGE_STACK_2MB_MAX_PAGES: usize = 262144;
const PAGE_STACK_1GB_MAX_PAGES: usize = 512;

const PAGE_AGGREGATOR_2MB_BASE: Address = 0xFFFFFFD040000000;
const PAGE_AGGREGATOR_1GB_BASE: Address = 0xFFFFFFE000200000;
const PAGE_AGGREGATOR_512GB_BASE: Address = 0xFFFFFFE000202000;

#[derive(PartialEq, Copy, Clone)]
pub enum PageType {
    Page4KB,
    Page2MB,
    Page1GB,
}

#[allow(dead_code)]
#[derive(PartialEq, Copy, Clone)]
pub enum SizedPage {
    Page4KB(Address),
    Page2MB(Address),
    Page1GB(Address),
}


pub fn physical_mirror<T>(addr: T) -> T where T: Into<Address> + From<Address> {
    let addr = addr.into();
    let mirrored = addr + PHYSICAL_OFFSET;
    mirrored.into()
}

pub fn pages_required(size: usize) -> usize {
    (size + (PAGE_SIZE_4KB - 1)) / PAGE_SIZE_4KB
}

pub fn get_pl4_table() -> &'static mut PageTable {
    unsafe {
        let (pl4_frame, _flags) = Cr3::read();
        &mut *(physical_mirror(pl4_frame.start_address().as_u64()) as *mut PageTable)
    }
}

pub fn invalidate_page(addr: VirtualAddress) {
    unsafe {
        core::arch::asm!(
            "invlpg 0({0:r})",
            in(reg) addr.0,
            options(nostack, preserves_flags, att_syntax)
        );
    }
}

pub fn set_mmio(virtual_addr: VirtualAddress) -> bool {
    if let Some(entry) = get_entry(virtual_addr) {
    
        let new_flags = entry.flags() | (
            PageTableFlags::WRITE_THROUGH & 
            PageTableFlags::NO_CACHE);

        entry.set_flags(new_flags);

        invalidate_page(virtual_addr);
        return true;
    }
    return false;
}

pub fn get_entry(virtual_addr: VirtualAddress) -> Option<&'static mut PageTableEntry> {
    let pl4_index = (virtual_addr.0 as usize >> 39) & 0o777;
    let pl3_index = (virtual_addr.0 as usize >> 30) & 0o777;
    let pl2_index = (virtual_addr.0 as usize >> 21) & 0o777;
    let pl1_index = (virtual_addr.0 as usize >> 12) & 0o777;

        let pl4_table = get_pl4_table();

        let pl4_entry = &mut pl4_table[pl4_index];
        if pl4_entry.is_unused() {
            return None;
        }

        // page directory entry is 4kb page...
        let pl3_table = entry_to_table(&pl4_entry);
        let pl3_entry = &mut pl3_table[pl3_index];
        if pl3_entry.is_unused() {
            return None;
        }

        if pl3_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
            return Some(pl3_entry);
        }

        // this could be a 2mb page...
        let pl2_table = entry_to_table(&pl3_entry);
        let pl2_entry = &mut pl2_table[pl2_index];
        if pl2_entry.is_unused() {
            return None;
        }

        if pl2_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
            return Some(pl2_entry);
        }

        let pl1_table = entry_to_table(&pl2_entry);
        let pl1_entry = &mut pl1_table[pl1_index];
        if pl1_entry.is_unused() {
            return None;
        }

        Some(pl1_entry)
}


fn entry_to_table(entry: &PageTableEntry) -> &'static mut PageTable {
    unsafe {
        &mut *(physical_mirror(entry.addr().as_u64()) as *mut PageTable)
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



pub fn run_time_tests(pager: &Pager) {
    // First test that the physical "itentity mapped" region exists at PHYSICAL_OFFSET
    let page = pager.allocate_page(PageType::Page4KB).expect("Must be able to acquire 4kb page");
    println!("Allocated page at 0x{:016x}", page);
    unsafe { core::ptr::write_bytes(physical_mirror(page) as *mut u8, 0xff, 4096); }

    let phys_test_addr = (page + PHYSICAL_OFFSET) as *const u8;
    let phys_test_array: &[u8; 4096] = unsafe {
        &*(phys_test_addr as *const [u8; 4096])
    };
    assert_eq!(phys_test_array[0], 0xff);

    unsafe { core::ptr::write_bytes(physical_mirror(page) as *mut u8, 0x80, 4096); }
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
    while let Some(mut page) = pager.allocate_4kb_page() {
        page += PHYSICAL_OFFSET;

        println!("Allocated 0x{:016x}", page);
        count += 1;

        if first_page == None {
            first_page = Some(page);
        } else {
            unsafe {
                // I technically *don't* need to add physical offset here, as the memory is still identity 
                // mapped in place, as well... but dereferencing address 0 will cause a panic... even though 
                // address 0 is actually mapped and valid...
                let last_page_ptr = last_page as *mut u64;
                *last_page_ptr = page;
            }
        }
        last_page = page;

        //yield_task();
    }

    println!("Allocated all pages; now freeing starting at {}", first_page.unwrap_or(0));


    // trace out the pages for debug...
    /*
    if let Some(mut address) = first_page {
        for _ in 0..count {
            println!("Addr 0x{:016x}", address);
            if address & (4096-1) != 0 {
                breakpoint();
            }
            // extract the next address before we free this page
            let next_page = unsafe {
                let next_page_ptr = address as *mut u64;
                *next_page_ptr
            };
            address = next_page;
        }
    }
    */

    // Now free them all back...
    if let Some(mut address) = first_page {
        for _ in 0..count {
            println!("Freeing 0x{:016x}", address);
            // extract the next address before we free this page
            let next_page = unsafe {
                let next_page_ptr = address as *mut u64;
                *next_page_ptr
            };
            pager.free_4kb_page(address - PHYSICAL_OFFSET);
            address = next_page;
        }
    }
}


#[allow(dead_code)]
fn breakpoint() {
    println!("Artificial breakpoint");
    loop {
        x86_64::instructions::hlt();
    }
}

pub fn get_kernel_cr3() -> u64 {
    X86_PAGER.get().unwrap().get_kernel_cr3()
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
        stack_1gb: Box< [Address] >,
        stack_2mb: Box< [Address] >,
        stack_4kb: Box< [Address] >,
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

            let stack_4kb_memory = vec![0u8; (total_4kb_stack_addresses * size_of::<Address>()) as usize].into_boxed_slice();
            let stack_2mb_memory = vec![0u8; (total_2mb_stack_addresses * size_of::<Address>()) as usize].into_boxed_slice();
            let stack_1gb_memory = vec![0u8; (total_1gb_stack_addresses * size_of::<Address>()) as usize].into_boxed_slice();

            let stack_4kb_memory_addr = (Box::into_raw(stack_4kb_memory) as *mut Address) as Address;
            let stack_2mb_memory_addr = (Box::into_raw(stack_2mb_memory) as *mut Address) as Address;
            let stack_1gb_memory_addr = (Box::into_raw(stack_1gb_memory) as *mut Address) as Address;
            let stack_4kb_memory_addr_top = stack_4kb_memory_addr + (total_4kb_stack_addresses * size_of::<Address>()) as Address;
            let stack_2mb_memory_addr_top = stack_2mb_memory_addr + (total_2mb_stack_addresses * size_of::<Address>()) as Address;
            let stack_1gb_memory_addr_top = stack_1gb_memory_addr + (total_1gb_stack_addresses * size_of::<Address>()) as Address;

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
                                    .step_by(PAGE_SIZE_1GB as usize)),
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
                                    .step_by(PAGE_SIZE_2MB as usize)),
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
                                    .step_by(PAGE_SIZE_4KB as usize)),
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
                stack_1gb: unsafe { Box::from_raw(core::slice::from_raw_parts_mut(stack_1gb_memory_addr as *mut Address, total_1gb_stack_addresses)) },
                stack_2mb: unsafe { Box::from_raw(core::slice::from_raw_parts_mut(stack_2mb_memory_addr as *mut Address, total_2mb_stack_addresses)) },
                stack_4kb: unsafe { Box::from_raw(core::slice::from_raw_parts_mut(stack_4kb_memory_addr as *mut Address, total_4kb_stack_addresses)) },
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