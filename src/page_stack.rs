//! Page Stack
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

use crate::stack::{Stack, ExpandUp, ExpandDown};
use crate::types::Address;

pub trait PageBorrower {
    fn borrow_pages(&mut self) -> Option< (Address, usize) >;
}

// TODO: this will be implemented by the pager, but will need some form of protection in order to 
// ensure it can be shared... need to implement some form of mutex
pub trait PageMapper {
    fn map_page(&mut self, page_addr: Address) -> bool;
    fn unmap_page(&mut self, page_addr: Address) -> bool;

    // or something like?
    // Ok(b) -> address is mapped, b == true means the page was mapped, otherwise it was already mapped
    // Err -> couldn't map the page..
    fn ensure_mapped(&mut self, page_addr: Address) -> Result<bool,()>;
}

pub struct PageStack< BORROWER: PageBorrower, MAPPER: PageMapper > {
    allocated_pages: Stack<'static, usize, ExpandDown>,
    available_pages: Stack<'static, usize, ExpandUp>,
    borrower: BORROWER,
    mapper: MAPPER,
}

impl<BORROWER: PageBorroer, MAPPER: PageMapper> PackStack<BORROWER, MAPPER> {
    pub fn new(borrower: BORROWER, mapper: MAPPER) -> PageStack<BORROWER, MAPPER> {
        // TODO determine where to place these stacks...
        // Allocate pages for the top and bottom of the stack, but leave the middle unmapped... it'll be mapped on demand
        // But the on-demand mapping will be intelligent (not requiring a page fault)
        PageStack {
            allocated_pages: Stack::new(0, 0),
            available_pages: Stack::new(0, 0),
            borrower,
            mapper,
        }
    }

    pub fn allocate_page(&mut self) -> Option<Address> {
        if self.available_pages.is_empty() {
            if let Some((page_addr, num_pages)) = self.borrower.borrow_pages() {
                for i in 0..num_pages {
                    self.available_pages.push(page_addr + (i * PAGE_SIZE));
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