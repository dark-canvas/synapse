use crate::pager::PhysicalAddress;
use crate::types::Address;

use super::{PageType, Pager, PHYSICAL_OFFSET};
use super::util::physical_mirror;

pub fn run_time_tests(pager: &Pager) {
    // First test that the physical "itentity mapped" region exists at PHYSICAL_OFFSET
    let page = pager.allocate_page(PageType::Page4KB).expect("Must be able to acquire 4kb page");
    println!("Allocated page at 0x{:016x}", page.0);
    unsafe { core::ptr::write_bytes(physical_mirror(page.0) as *mut u8, 0xff, 4096); }

    let phys_test_addr = (page.0 + PHYSICAL_OFFSET) as *const u8;
    let phys_test_array: &[u8; 4096] = unsafe {
        &*(phys_test_addr as *const [u8; 4096])
    };
    assert_eq!(phys_test_array[0], 0xff);

    unsafe { core::ptr::write_bytes(physical_mirror(page.0) as *mut u8, 0x80, 4096); }
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
        let page = page.0 + PHYSICAL_OFFSET;

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
            pager.free_4kb_page(PhysicalAddress(address - PHYSICAL_OFFSET));
            address = next_page;
        }
    }
}
