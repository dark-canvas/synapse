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

//! Instead of using an allocated stack, have an array indexes by the next largest page size.
//! The array contains the count of pages allocated/freed from that largest page size.
//!
//! 4kb page insdex == page / 2mb  == 512GB / 2MB == 262144 possible indexes
//!    array contains:
//!      u16 count of 4kb pages allocated from each 2mb page
//!      u16 count of 4kb pages available from each 2mb page
//!      total of both should always be 512 (and can be regularly asserted), and if the count of allocated pages goes to 0, 
//!      then the 2mb page can be returned to the 2mb stack and all the 4kb pages can be removed from the 4kb stack 
//!    if struct uses 4 byte entries == 256 pages, but only need to allocate enough for the actual total memory available
//!
//! 2mb page index == page / 1gb (512 possible 2mb pages in 512GB)
//! 1gb page stack doesn't have this array
//!
//! This means the page stack needs to know the highest physical address to create the above structs.


#[cfg(test)]
use mockall::*;
#[cfg(test)]
use mockall::predicate::*;

use crate::stack::{Stack, SimpleStack, EXPAND_UP};
use crate::types::Address;
use crate::pager::PageIterator;

use super::address_aggregator::{AddressAggregator, PageAggregator};

use super::PAGE_SIZE_1GB;
use super::PAGE_SIZE_4KB;
pub const ADDRESSES_PER_PAGE: usize = 512; // make this a const in pager?

#[derive(PartialEq, PartialOrd)]
enum Search {
    Continue(usize),
    Match(usize),
}

#[allow(dead_code)]
#[cfg_attr(test, automock)]
pub trait PageMapper {
    /// Ok(b) -> address is mapped, b == true means the page was mapped, otherwise it was already mapped
    /// Err -> couldn't map the page..
    fn ensure_mapped(&self, base: Address, end: Address) -> Result<bool, &'static str>;
}

// How to prevent double frees?
// How to prevent asymmetric free i.e., 
//   - allocate a 2mb page, but free it as a 4kb page?  Leaks memory)
//   - allocate a 4kb page, but free it as a 2mb page?  (frees memory that could be in use)
//     - iterate the page tables to ensure that the page in question is of the corret size before freeing
pub(crate) struct PageStack<const PAGE_SIZE: usize> 
where [(); PAGE_SIZE * ADDRESSES_PER_PAGE]: {
    pub(crate) available_pages: Stack<'static, Address, EXPAND_UP>,
    pub(crate) aggregate_map: PageAggregator< {PAGE_SIZE*ADDRESSES_PER_PAGE} >,
}

#[allow(dead_code)]
impl<const PAGE_SIZE: usize> PageStack<PAGE_SIZE> 
where [(); PAGE_SIZE * ADDRESSES_PER_PAGE] : {

    pub fn new<I: Iterator< Item = Address> > (stack_base: Address, page_count: usize, aggregator_base: Address, pages: I) -> Self 
    where [(); PAGE_SIZE * ADDRESSES_PER_PAGE] : {
        let mut available_pages = Stack::<Address, EXPAND_UP>::new(stack_base, page_count);
        let mut aggregate_map = PageAggregator::<{PAGE_SIZE * ADDRESSES_PER_PAGE}>::new(
                aggregator_base,
                (page_count + (ADDRESSES_PER_PAGE-1)) / ADDRESSES_PER_PAGE);

        for page in pages {
            available_pages.push(page);
            aggregate_map.mark_available(page);
        }
        PageStack {
            available_pages,
            aggregate_map,
        }
    }

    pub fn len(&self) -> usize {
        self.available_pages.len()
    }

    pub fn allocate_page(&mut self) -> Option<Address> 
    where [(); PAGE_SIZE * ADDRESSES_PER_PAGE] : {

        if self.available_pages.is_empty() {
            None
        } else {
            let page = self.available_pages.pop().unwrap();
            self.aggregate_map.allocate(page);
            Some(page)
        }
    }

    // NOTE: it is up to the caller to ensure this page has been previously allocated and is the 
    // proper size for this page stack.
    pub fn deallocate_page(&mut self, page_addr: Address) -> Option<Address>
    where [(); PAGE_SIZE * ADDRESSES_PER_PAGE] : {

        // align the page address to the page size, just in case
        let page_addr = page_addr & !(PAGE_SIZE as Address - 1);

        self.available_pages.push(page_addr);

        let mut result = None;
        if PAGE_SIZE != PAGE_SIZE_1GB {
            result = self.aggregate_map.deallocate(page_addr);

            if let Some(bigger_page) = result {
                let bigger_page_size = (PAGE_SIZE * ADDRESSES_PER_PAGE) as Address;

                let is_within = |addr: Address| -> bool {
                    addr >= bigger_page && addr < bigger_page + bigger_page_size
                };

                // we hvae all the pages which make up a larger page
                // remove them all from our stack
                // iterate the page stack from top and from bottom
                // from bottom seearches for pages to remove
                // from top searches for pages that belong (to swap with the ones from the bottom)
                let mut from_bottom_index = Search::Continue(0);
                let mut from_top_index = Search::Continue(self.available_pages.len() - 1);
                // TODO: get_unchecked() since we know it's within range?
                // Or create iterators for this purpose?
                while from_bottom_index < from_top_index {
                    from_bottom_index = match from_bottom_index {
                        Search::Continue(i) => {
                            if is_within(self.available_pages.get(i).unwrap()) {
                                Search::Match(i)
                            } else {
                                Search::Continue(i+1)
                            }
                        },
                        Search::Match(i) => Search::Match(i)
                    };

                    from_top_index = match from_top_index {
                        Search::Continue(i) => {
                            if !is_within(self.available_pages.get(i).unwrap()) {
                                Search::Match(i)
                            } else {
                                Search::Continue(i-1)
                            }
                        },
                        Search::Match(i) => Search::Match(i)
                    };

                    if let Search::Match(i) = from_bottom_index &&
                       let Search::Match(j) = from_top_index {
                        self.available_pages.swap(i, j);
                        from_bottom_index = Search::Continue(i+1);
                        from_top_index = Search::Continue(j-1);
                    }
                }
                self.available_pages.truncate(self.available_pages.len() - ADDRESSES_PER_PAGE);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;
    use std::vec::Vec;
    use std::collections::HashSet;
    use crate::pager::PAGE_SIZE_2MB;
    use crate::pager::address_aggregator::PageBucket;

    #[test]
    fn test_create_stack() {
        let address_stack = [0 as Address; 10];
        let address_aggregator = [PageBucket{allocated: 0, available: 0}; 4096];

        let addresses = vec![ 0x3000 as Address, 0x4000, 0x5000 ];

        let ps = PageStack::<4096>::new(
            address_stack.as_ptr() as Address, 
            10 * ADDRESSES_PER_PAGE, 
            address_aggregator.as_ptr() as Address,
            addresses.iter().copied());

        assert_eq!(address_stack[0], 0x3000);
        assert_eq!(address_stack[1], 0x4000);
        assert_eq!(address_stack[2], 0x5000);
        assert_eq!(address_aggregator[0].available, 3);
    }

    #[test]
    fn test_allocate_page() {
        let address_stack = [0 as Address; 10];
        let mut address_aggregator = [PageBucket{allocated: 0, available: 0}; 4096];

        let address1 = 0x1234000 as Address;
        let address2 = 0x5678000 as Address;

        let addresses = vec![ address1, address2 ];

        let max_pages = address2 as usize / 4096;

        let mut ps = PageStack::<4096>::new(
            address_stack.as_ptr() as Address, 
            max_pages, 
            address_aggregator.as_ptr() as Address,
            addresses.iter().copied());

        assert_eq!(ps.allocate_page(), Some(address2));
        assert_eq!(ps.allocate_page(), Some(address1));
        assert_eq!(ps.allocate_page(), None);
    }

    #[test]
    fn test_deallocate_page() {
        let address_stack = [0 as Address; 10];
        let mut address_aggregator = [PageBucket{allocated: 0, available: 0}; 4096];

        let addresses = vec![ ];

        let address1 = 0x1234000 as Address;
        let address2 = 0x5678000 as Address;

        let max_pages = address2 as usize / 4096;

        let mut ps = PageStack::<4096>::new(
            address_stack.as_ptr() as Address, 
            max_pages, 
            address_aggregator.as_ptr() as Address,
            addresses.iter().copied());

        address_aggregator[address1 as usize / PAGE_SIZE_2MB].allocated = 5;
        address_aggregator[address2 as usize / PAGE_SIZE_2MB].allocated = 10;

        assert_eq!(ps.deallocate_page(address1), None);
        assert_eq!(ps.deallocate_page(address2), None);

        assert_eq!(address_stack[0], 0x1234000);
        assert_eq!(address_stack[1], 0x5678000);
        assert_eq!(address_aggregator[address1 as usize / PAGE_SIZE_2MB].allocated, 4);
        assert_eq!(address_aggregator[address1 as usize / PAGE_SIZE_2MB].available, 1);
        assert_eq!(address_aggregator[address2 as usize / PAGE_SIZE_2MB].allocated, 9);
        assert_eq!(address_aggregator[address2 as usize / PAGE_SIZE_2MB].available, 1);
    }
    
    // test that returning/deallocating all pages back to the stack results in them being 
    // aggregated back to a larger page
    #[test]
    fn test_deallocation_aggregation() {
        let address_stack = [0 as Address; 512]; // just enough space to hold them all
        let mut address_aggregator = [PageBucket{allocated: 0, available: 0}; 4096];

        // Start off with all 4kb addresses which comprise the 2mb page from 10MB to 2MB
        // i.e., the 5th 2mb page (first being the 0th)
        let mut addresses = Vec::<Address>::new();
        let base_address = (PAGE_SIZE_2MB*5) as Address;
        let top_of_stack = base_address + (PAGE_SIZE_2MB - PAGE_SIZE_4KB) as Address;
        for i in (0..PAGE_SIZE_2MB).step_by(PAGE_SIZE_4KB) {
            addresses.push(base_address + i as Address)
        }
        // no sense continuing if this isn't true...
        assert_eq!(addresses.len(), ADDRESSES_PER_PAGE);

        let max_pages = (PAGE_SIZE_2MB * 6) / 4096;

        let mut ps = PageStack::<4096>::new(
            address_stack.as_ptr() as Address, 
            max_pages, 
            address_aggregator.as_ptr() as Address,
            addresses.iter().copied());

        // we expect the stack to be constructed with all the addresses...
        assert_eq!(address_stack[0], base_address);
        assert_eq!(address_stack[511], top_of_stack);
        assert_eq!(ps.len(), ADDRESSES_PER_PAGE);

        // now if we allocate a page, and return it back (deallocate it) the stack should realize it has 
        // enough pages to aggregate into a single 2mb page, and should tell us that...
        assert_eq!(ps.allocate_page(), Some(top_of_stack));
        assert_eq!(ps.deallocate_page(top_of_stack), Some(base_address));
        assert_eq!(ps.len(), 0);
    }

    // test that the stack has all pages removed that belong to the aggregated page
    #[test]
    fn test_deallocation_aggregation_stack_cleanup() {
        let address_stack = [0 as Address; 1024]; // just enough space to hold them all
        let mut address_aggregator = [PageBucket{allocated: 0, available: 0}; 4096];

        // intersperse 4kb pages belonging to two different 2mb pages
        let mut addresses = Vec::<Address>::new();
        let base_address_aggregate_page = (PAGE_SIZE_2MB*5) as Address;
        let base_address_other_page = (PAGE_SIZE_2MB*2) as Address;
        let top_of_stack = base_address_aggregate_page + (PAGE_SIZE_2MB - PAGE_SIZE_4KB) as Address;

        for i in 0..512 {
            addresses.push(base_address_other_page + (i* PAGE_SIZE_4KB) as Address);
            addresses.push(base_address_aggregate_page + (i * PAGE_SIZE_4KB) as Address);
        }
        // no sense continuing if this isn't true...
        assert_eq!(addresses.len(), ADDRESSES_PER_PAGE*2);

        let max_pages = (PAGE_SIZE_2MB * 6) / 4096;
        let mut ps = PageStack::<4096>::new(
            address_stack.as_ptr() as Address, 
            max_pages, 
            address_aggregator.as_ptr() as Address,
            addresses.iter().copied());

        // we expect the stack to be constructed with all the addresses...
        assert_eq!(ps.len(), addresses.len());

        // now if we allocate a page, and return it back (deallocate it) the stack should realize it has 
        // enough pages to aggregate into a single 2mb page, and should tell us that...
        assert_eq!(ps.allocate_page(), Some(top_of_stack));
        assert_eq!(ps.deallocate_page(top_of_stack), Some(base_address_aggregate_page));
        assert_eq!(ps.len(), 512);

        // Now allocate pages and ensure they're all within range of the un-aggregated 
        // page and that they're all unique
        let mut address_set = HashSet::new();
        while ps.len() > 0 {
            if let Some(page) = ps.allocate_page() {
                address_set.insert(page);
            } else {
                assert!(false);
            }
        }
        assert_eq!(address_set.len(), 512);
    }
}
