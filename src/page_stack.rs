//! Page Stack
//!
//! A structure for managing allocation/freeing of varoius different page sizes.
//!
//! == Overview ==
//!
//! Implements a page management structure for each of the different page sizes, whereby 
//! smaller page sizes can borrow a page from a larger allocater if/when it runs out.
//!
//! An example of the relationship appears below, whereby the 2mb stack was previously 
//! empty, so it "borrowed" a page from the next allocater up in size (the 1gb allocated) 
//! which allocated a 1gb page, and distributed it as 512 2mb pages to the 2mb allocator.
//!
//! In this example, the 4kb allocater has a few pages allocated, likely to form page 
//! tables, and is otherwise empty.  If someone were to request a 4kb page, the allocator 
//! would need to borrow a page from the 2mb allocator and, in return, would acquire 512 
//! 4kb pages.
//!
//! ```
//! --------------    --------------    --------------
//! 4kb Page stack    2mb Page Stack    1gb Page Stack
//! --------------    --------------    --------------
//!  Allocated 0        Aloocated 0     - Allocated 0
//!  Allocated 1                       |
//!     ...                            |
//!  Allocated N                       |
//!                                    |
//!                                    |
//!                                    |
//!                     Available N \  |
//!                         ,,,     |  /
//!                     Available 2 |/
//!                     Available 1 |
//!                     Available 0 /
//! ```
//!
//! TODO: need an efficient way to determine if/when all 512 pages from a smaller 
//! allocator can be returns as a simple big page to the larger allocator.
//!
//! NOTE: Most modern AMD64 processors support a 48-bit or 43-bit physical address space.
//! This would mean that, in order to support the full address space:
//!  - the 4kb page stack would occupy 2^48 / 2^12 * 8 == 549755813888 (512GB)
//!  - the 2mb page stack would occupy 2^48 / 2^21 * 8 == 1073741824   (  1GB)
//!  - the 1gb page stack would occupy 2^48 / 2^30 * 8 == 2097152      (  2MB)
//! That's over 1.5GB overhead just to account for every possible way that memory could be 
//! allocated.
//! This value grows even more if a processor supports the fully 52-bit address space 
//! which is possible.
//!
//! Rather than outright allocating space for memory allocation possibilities which are 
//! unlikely to happen, we can fill the most efficient page stack (the 1gb stack) and map 
//! only the minimum pages required to represent it.
//! The 2mb stack could be completed empty (except, perhaps, for the actual kernel code 
//! and data which has been loaded).
//! The 4kb stack would also be largely empty, holding only the allocated pages used to 
//! build the page tables.
//! All un-used memory (i.e., between the tops of the used and allocated stacks) would 
//! remain unmapped, and only mapped into place when required (although the pages can 
//! be intelligently mapped, rather than requiring the overhead of a page fault)

use mockall::*;
use mockall::predicate::*;

use crate::stack::{Stack, EXPAND_UP, EXPAND_DOWN};
use crate::types::Address;

use core::ptr;

// These traits must use interior mutability to accomplish their goals, as the 
// interfaces are intentionally non-mut

#[automock]
pub trait PageBorrower {
    fn borrow_pages(&self) -> Option< (Address, usize) >;
}

// TODO: this will be implemented by the pager, but will need some form of protection in order to 
// ensure it can be shared... need to implement some form of mutex
#[automock]
pub trait PageMapper {
    //fn map_page(&mut self, page_addr: Address) -> bool;
    //fn unmap_page(&mut self, page_addr: Address) -> bool;

    // or something like?
    // Ok(b) -> address is mapped, b == true means the page was mapped, otherwise it was already mapped
    // Err -> couldn't map the page..
    fn ensure_mapped(&self, base: Address, end: Address) -> Result<bool,()>;
}

pub struct PageStack<'a, BORROWER: PageBorrower, MAPPER: PageMapper, const PAGE_SIZE: usize > {
    allocated_pages: Stack<'static, Address, EXPAND_DOWN>,
    available_pages: Stack<'static, Address, EXPAND_UP>,
    borrower: &'a BORROWER,
    mapper: &'a MAPPER,
}

impl<'a, BORROWER: PageBorrower, MAPPER: PageMapper, const PAGE_SIZE: usize> PageStack<'a, BORROWER, MAPPER, PAGE_SIZE> {
    pub fn new(borrower: &'a BORROWER, mapper: &'a MAPPER, stack_base: Address, page_count: usize) -> PageStack<'a, BORROWER, MAPPER, PAGE_SIZE> {
        // TODO determine where to place these stacks...
        // Allocate pages for the top and bottom of the stack, but leave the middle unmapped... it'll be mapped on demand
        // But the on-demand mapping will be intelligent (not requiring a page fault)
        // NOTE that `page_count` is the maximum number of pages, and the size of the underlying memory, but is supplied as the 
        // size of both the allocated and available stacks, which means they technically overlap memory, but becaues a page can 
        // only be in one stack at the same time, they'll never actually collide with each other
        PageStack {
            allocated_pages: Stack::<Address, EXPAND_DOWN>::new(stack_base + (page_count * size_of::<Address>()) as Address, page_count),
            available_pages: Stack::<Address, EXPAND_UP>::new(stack_base, page_count),
            borrower,
            mapper,
        }
    }

    pub fn allocate_page(&mut self) -> Option<Address> {
        if self.available_pages.is_empty() {
            if let Some((page_addr, num_pages)) = self.borrower.borrow_pages() {
                let top_of_stack = self.available_pages.top();
                let new_top = top_of_stack + (num_pages * size_of::<Address>()) as Address;
                self.mapper.ensure_mapped(top_of_stack, new_top);

                for i in 0..num_pages {
                    self.available_pages.push(page_addr + (i * PAGE_SIZE) as Address);
                }
            } else {
                return None; // No more pages available
            }
        }

        self.allocated_pages.push(self.available_pages.pop().unwrap());
        Some(self.allocated_pages.top())
    }

    pub fn deallocate_page(&mut self, page_addr: Address) -> Option<Address> {
        // TODO: page align the address
        // TODO: confirm it was actually allocated

        if self.allocated_pages.is_empty() {
            return None; // No pages to deallocate
        }

        let page_addr = self.allocated_pages.pop().unwrap();
        self.available_pages.push(page_addr);
        Some(page_addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_stack() {
        let borrower = MockPageBorrower::new();
        let mapper = MockPageMapper::new();

        let ps = PageStack::<_, _, 4096>::new(&borrower, &mapper, 0x1000, 0);
    }

    #[test]
    fn test_borrow() {
        const pages: usize = 10;
        const guard_band: u64 = 0x1122334455667788;

        let mut borrower = MockPageBorrower::new();
        let mut mapper = MockPageMapper::new();
        let mut stack_memory = [0u64; pages];
        let stack_memory_base = stack_memory.as_ptr() as Address;
        let stack_end_after_additions = ptr::addr_of!(stack_memory[4]) as Address;

        mapper.expect_ensure_mapped()
            .times(1)
            .with(predicate::eq(stack_memory_base), predicate::eq(stack_end_after_additions))
            .return_const(Ok(false));

        borrower.expect_borrow_pages()
            .times(1)
            .return_const(Some((0x1000, 4)));

        let mut ps = PageStack::<_, _, 4096>::new(&borrower, &mapper, stack_memory_base, pages);

        stack_memory[4] = guard_band;
        ps.allocate_page();

        assert_eq!(stack_memory[0], 0x1000);
        assert_eq!(stack_memory[1], 0x2000);
        assert_eq!(stack_memory[2], 0x3000);
        assert_eq!(stack_memory[3], 0x4000);
        assert_eq!(stack_memory[4], guard_band)
    }

}