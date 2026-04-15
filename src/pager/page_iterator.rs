use crate::types::Address;
use satus_struct::memory_map::{MemoryMap, MemoryRegionType};

use super::{PAGE_SIZE_4KB, PAGE_SIZE_2MB, PAGE_SIZE_1GB};
use super::{is_1gb_aligned, is_2mb_aligned, is_4kb_aligned};
use super::{next_1gb_page, next_2mb_page, next_4kb_page};

/// A struct that can be used to iterate over an mmap and return memory in its largest possible page sizes, 
/// while also allowing for filtering by region type and base address, and excluding specific page ranges 
/// (e.g., for the page stack itself)
pub struct PageIterator<'a> {
    mmap: &'a MemoryMap,
    page_size: Address,
    current_region: usize,
    current_page: Option<Address>,
    region_type: Option<MemoryRegionType>,
    base_address: Option<Address>,
    exclude_page_range: Option< (Address, Address) >
}

impl<'a> PageIterator<'a> {
    pub fn new(mmap: &'a MemoryMap, page_size: usize) -> Self {
        PageIterator{
            mmap,
            page_size: page_size as Address,
            current_region: 0,
            current_page: None,
            region_type: None,
            base_address: None,
            exclude_page_range: None,
        }
    }

    pub fn with_region_type(mut self, region_type: MemoryRegionType) -> Self {
        self.region_type = Some(region_type);
        self
    }

    pub fn with_base_address(mut self, base_address: Address) -> Self {
        self.base_address = Some(base_address);
        self
    }

    pub fn excluding_range(mut self, start: Address, end: Address) -> Self {
        self.exclude_page_range = Some((start, end));
        self
    }

    fn passes_filters(&self, addr: Address) -> bool {
        if let Some(base_address) = self.base_address {
            if addr < base_address {
                return false;
            }
        }

        if let Some(exclude_page_range) = self.exclude_page_range {
            if addr >= exclude_page_range.0 && addr < exclude_page_range.1 {
                return false;
            }
        }

        true
    }

    pub fn get_count(&self) -> usize {
        // iterate all the pages, without editing our internal state
        let mut current_region = 0;
        let mut current_page : Option<Address> = None;
        let mut count = 0;
        loop {
            let (region, page, result) = self.iterate(current_region, current_page);
            if result == None {
                break;
            }
            count += 1;
            current_region = region;
            current_page = Some(page);
        }
        count
    }

    pub fn get_current(&self) -> Option<Address> {
        self.current_page
    }

    fn iterate(&self, current_region: usize, start_page: Option<Address>) -> (usize, Address, Option<Address>) {
        let mut start_page = start_page;
        let num_regions = self.mmap.get_num_regions();

        for i in current_region..num_regions {
            let region = self.mmap.get_memory_region(i).unwrap();

            if let Some(region_type) = self.region_type {
                if region.get_type() != region_type {
                    continue;
                }
            }

            let (start, end) = region.get_address_range();
            let mut current = match start_page {
                Some(sp) => { start_page = None; sp },
                None => start,
            };
        
            // if a base address was specified, skip past any regions before it
            if let Some(base_address) = self.base_address {
                if end < base_address {
                    continue;
                }
            }

            while current < end  && current + PAGE_SIZE_4KB as Address <= end {
                // skip past any 1gb pages
                while is_1gb_aligned(current) && next_1gb_page(current) <= end {
                    let next = next_1gb_page(current);
                    if self.page_size == PAGE_SIZE_1GB as Address && self.passes_filters(current) {
                        return (i, next, Some(current));
                    }
                    current = next;
                }

                // skip past any 2mb pages
                // if we find a 1GB aligned page we must break and restart...
                while is_2mb_aligned(current) && next_2mb_page(current) <= end {
                    if is_1gb_aligned(current) && next_1gb_page(current) <= end {
                        break;
                    }

                    let next = next_2mb_page(current);
                    if self.page_size == PAGE_SIZE_2MB as Address && self.passes_filters(current) {
                        return (i, next, Some(current));
                    }
                    current = next;
                }

                while is_4kb_aligned(current) && next_4kb_page(current) <= end {
                    if is_2mb_aligned(current) && next_2mb_page(current) <= end {
                        break;
                    }

                    let next = next_4kb_page(current);
                    if self.page_size == PAGE_SIZE_4KB as Address && self.passes_filters(current){
                        return (i, next, Some(current));
                    }
                    current = next;
                }
            }
        }
        return (0, 0, None);
    }
}

// Implement the Iterator trait
impl<'a> Iterator for PageIterator<'a> {
    // Specify the type of item the iterator yields
    type Item = Address;

    fn next(&mut self) -> Option<Self::Item> {
        let (region, page, result) = self.iterate(self.current_region, self.current_page);
        self.current_region = region;
        self.current_page = Some(page);
        result
    }
}
