//! Address Aggregator
//!
//! Tracks pages allocated/available from larger page sizes.
//! 
//! This struct is used to determine when all the pages from a larger page have been returned,
//! so the larger page can be returned to the larger stack.

use crate::types::Address;

#[derive(Copy, Clone)]
pub struct PageBucket {
    pub allocated: u16,
    pub available: u16,
}

#[allow(dead_code)]
pub trait AddressAggregator {
    fn mark_available(&mut self, page_addr: Address);
    fn mark_allocated(&mut self, page_addr: Address);
    fn allocate(&mut self, page_addr: Address);
    fn deallocate(&mut self, page_addr: Address) -> Option<Address>;
}

/// PageAggregator tracks the pages which have been allocated from a larger page, and how many are still available.
/// This allows us to determine when all the pages from a larger page have been returned, and we can return 
/// the larger page to the larger stack.
/// The aggregate map is indexed by the next largest page size, so for 4kb pages, it's indexed by 2mb pages, 
/// and for 2mb pages, it's indexed by 1gb pages.
pub struct PageAggregator<const PAGE_SIZE: usize> {
    pub aggregate_map: &'static mut [PageBucket],
}

impl<const PAGE_SIZE: usize>  PageAggregator<PAGE_SIZE> {
    pub fn new(aggregate_map_base: Address, num_buckets: usize) -> Self {
        unsafe {
            core::ptr::write_bytes(aggregate_map_base as *mut u8, 0, core::mem::size_of::<PageBucket>() * num_buckets);
        }
        PageAggregator {
            aggregate_map: unsafe { core::slice::from_raw_parts_mut(aggregate_map_base as *mut PageBucket, num_buckets) }
        }
    }
}

impl <const PAGE_SIZE: usize> AddressAggregator for PageAggregator<PAGE_SIZE> {

    fn mark_available(&mut self, page_addr: Address) {
        let agg_index = (page_addr as usize / PAGE_SIZE) as usize;
        self.aggregate_map[agg_index].available += 1;
    }

    fn mark_allocated(&mut self, page_addr: Address) {
        let agg_index = (page_addr as usize / PAGE_SIZE) as usize;
        self.aggregate_map[agg_index].allocated += 1;
    }

    fn allocate(&mut self, page_addr: Address) {
        let agg_index = page_addr as usize / PAGE_SIZE;
        self.aggregate_map[agg_index].allocated += 1;
        self.aggregate_map[agg_index].available -= 1;
    }

    fn deallocate(&mut self, page_addr: Address) -> Option<Address> {
        let agg_index = page_addr as usize / PAGE_SIZE;
        self.aggregate_map[agg_index].allocated -= 1;
        self.aggregate_map[agg_index].available += 1;

        if self.aggregate_map[agg_index].allocated == 0 {
            // we hvae all the pages which make up a larger page
            // remove them all from our stack 
            self.aggregate_map[agg_index].available = 0;
            // and indicate the can be returned to the larger stack
            let agg_page_addr = (agg_index as Address) * PAGE_SIZE as Address;
            return Some(agg_page_addr);
        }
        None
    }
}
