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

use core::ptr;
use crate::stack::{Stack, SimpleStack, EXPAND_UP, EXPAND_DOWN};
use crate::types::Address;

use spin::Mutex;
use super::address_aggregator::{AddressAggregator, PageAggregator, PageBucket};

use super::PAGE_SIZE_1GB;
pub const ADDRESSES_PER_PAGE: usize = 512; // make this a const in pager?

#[derive(PartialEq, PartialOrd)]
enum Search {
    Continue(usize),
    Match(usize),
}
// These traits must use interior mutability to accomplish their goals, as the 
// interfaces are intentionally non-mut

#[cfg_attr(test, automock)]
pub trait PageBorrower {
    // borrow a page from another source
    // Returns a address, representing a page, and the size of the page
    fn borrow_page(&self) -> Option< (Address, usize) >;

    fn borrow_specific(&self, address: Address) -> Option< (Address, usize) >;
}

// TODO: this will be implemented by the pager, but will need some form of protection in order to 
// ensure it can be shared... need to implement some form of mutex

#[cfg_attr(test, automock)]
pub trait PageMapper {
    /// Ok(b) -> address is mapped, b == true means the page was mapped, otherwise it was already mapped
    /// Err -> couldn't map the page..
    fn ensure_mapped(&self, base: Address, end: Address) -> Result<bool,(&'static str)>;
}

pub struct Stacks<const PAGE_SIZE: usize> 
where [(); {PAGE_SIZE*ADDRESSES_PER_PAGE}] : {
    //pub /* TODO (in crate::pager)*/ allocated_pages: Stack<'static, Address, EXPAND_DOWN>,
    pub /* TODO (in crate::pager)*/ available_pages: Stack<'static, Address, EXPAND_UP>,
    pub /* TODO (in crate::pager)*/ aggregate_map: PageAggregator< {PAGE_SIZE*ADDRESSES_PER_PAGE} >,
}

// TODO: this should be private
impl<const PAGE_SIZE: usize> Stacks<PAGE_SIZE>
where [(); {PAGE_SIZE*ADDRESSES_PER_PAGE}]: {
    fn new(stack_base: Address, page_count: usize, aggregator_base: Address) -> Self 
    where [(); {PAGE_SIZE * ADDRESSES_PER_PAGE}] : {
        Stacks {
            //allocated_pages: Stack::<Address, EXPAND_DOWN>::new(stack_base + (page_count * size_of::<Address>()) as Address, page_count),
            available_pages: Stack::<Address, EXPAND_UP>::new(stack_base, page_count),
            aggregate_map: PageAggregator::<{PAGE_SIZE * ADDRESSES_PER_PAGE}>::new(
                aggregator_base,
                page_count / ADDRESSES_PER_PAGE),

        }
    }
}




// need a mutex to wrap allocated/available pages... can it wrap both?  
// Or separate mutexes for each, and use `allocated_pages` only for debug?  (disable for production)
// How to prevent double frees?
// How to prevent asymmetric free i.e., 
//   - allocate a 2mb page, but free it as a 4kb page?  Leaks memory)
//   - allocate a 4kb page, but free it as a 2mb page?  (frees memory that could be in use)
//     - iterate the page tables to ensure that the page in question is of the corret size before freeing
pub struct PageStack<const PAGE_SIZE: usize> where [(); {PAGE_SIZE * ADDRESSES_PER_PAGE}]: {
    // Do we even need to track allocated pages?  If available pages is kept sorted (which could be a requirement to 
    // return larger pages to larger pack stacks) then determining double frees is just a matter of ensuring it isn't 
    // already in the available stack.
    // Also keeping available pages sorted allows for merging of adjacent pages into larger pages when they are returned to the stack.
    // Ultimately, determining/preventing double frees should actually be on the programmer, not the OS... it *SHOULD* 
    // be preventable without additionala support from the OS.
    // wrap the whole thing in a mutex?
    // Move the mutex to the caller?
    pub /* TODO (in crate::pager)*/ stacks: Mutex< Stacks<PAGE_SIZE> >,
}

impl<const PAGE_SIZE: usize> PageStack<PAGE_SIZE> 
where [(); {PAGE_SIZE * ADDRESSES_PER_PAGE}] : {

    pub fn new(stack_base: Address, page_count: usize, aggregator_base: Address) -> Self 
    where [(); {PAGE_SIZE * ADDRESSES_PER_PAGE}] : {
        // TODO determine where to place these stacks...
        // Allocate pages for the top and bottom of the stack, but leave the middle unmapped... it'll be mapped on demand
        // But the on-demand mapping will be intelligent (not requiring a page fault)
        // NOTE that `page_count` is the maximum number of pages, and the size of the underlying memory, but is supplied as the 
        // size of both the allocated and available stacks, which means they technically overlap memory, but becaues a page can 
        // only be in one stack at the same time, they'll never actually collide with each other
        PageStack {
            stacks: Mutex::new(Stacks::new(stack_base, page_count, aggregator_base)),
            //borrower: None,
            //mapper: mapper,
        }
    }

    // TODO: mapper isn't currently used... the intent was to allow for the memory backing the 
    // page stack itself to be dynamically mapped into place
    // Although; it would never be used in this method since we're just decreasing the available stack
    // Arguably it could return unuesd pages.
    pub fn allocate_page(&self, mapper: &dyn PageMapper) -> Option<Address> 
    where [(); {PAGE_SIZE * ADDRESSES_PER_PAGE}] : {

        let mut stacks_lock = self.stacks.lock();
        let stacks = &mut stacks_lock;
        /*
        if stacks.available_pages.is_empty() {
            // TOOD: borrow page...
            if let Some(borrower) = self.borrower {
                if let Some((page_addr, page_size)) = borrower.borrow_page() {
                    let num_pages = page_size / PAGE_SIZE;
                    let top_of_stack = stacks.available_pages.top();
                    let new_top = top_of_stack + (num_pages * size_of::<Address>()) as Address;

                    // TODO: if this calls into the pager, it could allocate new 4kb pages, which could be 
                    // calling back into this very same page stack (creating a dead lock?  Depends on whether spinlock is recursive)
                    // self.mapper.ensure_mapped(top_of_stack, new_top).unwrap();

                    for i in 0..num_pages {
                        stacks.available_pages.push(page_addr + (i * PAGE_SIZE) as Address);
                    }
                }
            }
        }
            */
        //drop(available_pages_lock);

        if stacks.available_pages.is_empty() {
            None
        } else {
            let page = stacks.available_pages.pop().unwrap();
            stacks.aggregate_map.allocate(page);
            // allocated is an expand down stack...
            //let top_of_stack = stacks.allocated_pages.top();
            //let new_top = top_of_stack - size_of::<Address>() as Address;
            //mapper.ensure_mapped(new_top, top_of_stack).unwrap();
            //stacks.allocated_pages.push(page);
            Some(page)
        }
    }

    /*
    pub fn allocate_specific(&mut self, page_addr: Address) -> Option<Address> {
        // TODO: ensure alignment of page_addr

        // check if this page is in the available portion of our stack
        // if not, try to borrow it (which will result in other pages being returned)
        let mut stacks_lock = self.stacks.lock();
        let stacks = &mut stacks_lock;
        if let Some(index) = stacks.available_pages.find(page_addr) {
            stacks.available_pages.remove_index(index);
            stacks.allocated_pages.push(page_addr);
            return Some(page_addr);
        } else if let Some(borrower) = self.borrower {
            if let Some( (base_address, size) ) = borrower.borrow_specific(page_addr) {
                let num_pages = size/PAGE_SIZE;
                // TODO: ensure mapped
                // self.mapper.ensure_mapped(base_address, base_address + size).unwrap();
                for i in 0..num_pages {
                    let new_page = base_address + (i * PAGE_SIZE) as Address;
                    if new_page != page_addr {
                        stacks.available_pages.push(new_page);
                    }
                }
            }
        }

        None
    }
        */

    // NOTE: it is up to the caller to ensure this page hsa been previously allocated and is the 
    // proper size for this page stack.
    pub fn deallocate_page(&self, page_addr: Address) -> Option<Address>
    where [(); {PAGE_SIZE * ADDRESSES_PER_PAGE}] : {

        // TODO: page align the address)
        let mut stacks_lock = self.stacks.lock();
        let stacks = &mut stacks_lock;

        stacks.available_pages.push(page_addr);

        let mut result = None;
        if PAGE_SIZE != PAGE_SIZE_1GB {
            result = stacks.aggregate_map.deallocate(page_addr);

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
                let mut from_top_index = Search::Continue(stacks.available_pages.len() - 1);
                // TODO: get_unchecked() since we know it's within range?
                // Or create iterators for this purpose?
                while from_bottom_index < from_top_index {
                    from_bottom_index = match from_bottom_index {
                        Search::Continue(i) => {
                            if is_within(stacks.available_pages.get(i).unwrap()) {
                                Search::Match(i)
                            } else {
                                Search::Continue(i+1)
                            }
                        },
                        Search::Match(i) => Search::Match(i)
                    };

                    from_top_index = match from_top_index {
                        Search::Continue(i) => {
                            if !is_within(stacks.available_pages.get(i).unwrap()) {
                                Search::Match(i)
                            } else {
                                Search::Continue(i-1)
                            }
                        },
                        Search::Match(i) => Search::Match(i)
                    };

                    if let Search::Match(i) = from_bottom_index &&
                       let Search::Match(j) = from_top_index {
                        stacks.available_pages.swap(i, j);
                        from_bottom_index = Search::Continue(i+1);
                        from_top_index = Search::Continue(j-1);
                    }
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_stack() {
        //let borrower = MockPageBorrower::new();
        //let mapper = MockPageMapper::new();

        let ps = PageStack::<4096>::new(0x1000, 0, 0x2000);
        // TODO: what can we assert here?
    }

    /*
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

        borrower.expect_borrow_page()
            .times(1)
            .return_const(Some((0x1000, 4*4096)));

        let mut ps = PageStack::< _, 4096>::new(mapper, stack_memory_base, pages);
        ps.set_borrow_source(&borrower);

        stack_memory[4] = guard_band;
        ps.allocate_page();

        assert_eq!(stack_memory[0], 0x1000);
        assert_eq!(stack_memory[1], 0x2000);
        assert_eq!(stack_memory[2], 0x3000);
        assert_eq!(stack_memory[3], 0x4000);
        assert_eq!(stack_memory[4], guard_band)
    }
    */

    #[test]
    fn test_aggregator() {
        let agg_data = [PageBucket{ allocated:0, available:0 }; 10];
        let mut aggregator = PageAggregator::<1000>::new(agg_data.as_ptr() as Address, agg_data.len());

        // Creating the aggregator will clear the memory... fill up buckets
        for i in 0..agg_data.len() {
            aggregator.aggregate_map[i].allocated = 0;
            aggregator.aggregate_map[i].available = 512;
        }

        aggregator.allocate(0);
        assert_eq!(aggregator.aggregate_map[0].allocated, 1);
        assert_eq!(aggregator.aggregate_map[0].available, 511);

        aggregator.allocate(100);
        assert_eq!(aggregator.aggregate_map[0].allocated, 2);
        assert_eq!(aggregator.aggregate_map[0].available, 510);

        aggregator.allocate(5123);
        assert_eq!(aggregator.aggregate_map[5].allocated, 1);
        assert_eq!(aggregator.aggregate_map[5].available, 511);

        let result = aggregator.deallocate(5999);
        assert_eq!(aggregator.aggregate_map[5].allocated, 0);
        assert_eq!(aggregator.aggregate_map[5].available, 0);
        assert_eq!(result, Some(5000));
    }
}
