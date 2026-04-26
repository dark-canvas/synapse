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

#[cfg(test)]
mod tests {
    use super::*;
    use satus_struct::memory_map::MemoryRegionType;

    fn pages_4kb(n: usize) -> Address { (PAGE_SIZE_4KB * n) as Address }
    fn pages_2mb(n: usize) -> Address { (PAGE_SIZE_2MB * n) as Address }
    fn pages_1gb(n: usize) -> Address { (PAGE_SIZE_1GB * n) as Address }

    #[test]
    fn test_page_iterator() {
        // create a mmap with various edge cases
        let mmap_page = [0u8; 4096];
        let mmap_page_addr = mmap_page.as_ptr() as Address;
        let mut mmap = MemoryMap::new_from_page(mmap_page_addr).unwrap();

        let mut base: Address = 0;

        // 1. multiple 4kb pages in a region
        mmap.add_region(MemoryRegionType::Available, base, base + pages_4kb(2));

        // 2. a single 4kb page in a region
        base = pages_4kb(4);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_4kb(1));

        // 3. multiple 2mb pages in a region
        base = PAGE_SIZE_2MB as Address;
        mmap.add_region(MemoryRegionType::Available, base, base + pages_2mb(2));

        // 4. a single 2mb page in a region
        base = pages_2mb(4);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_2mb(1));

        // 5. multiple 1gb pages in a region
        base = PAGE_SIZE_1GB as Address;
        mmap.add_region(MemoryRegionType::Available, base, base + pages_1gb(3));

        // 6. a single 1gb page in a region
        base = pages_1gb(4);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_1gb(1));

        // 7. some 4kb pages followed by a 2mb page
        base = pages_1gb(6) - pages_4kb(2);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_4kb(2) + pages_2mb(1));

        // 8. some 4kb pages followed by a 1gb page
        base = pages_1gb(7) - pages_4kb(2);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_4kb(2) + pages_1gb(1));

        // 9. some 2mb pages followed by a 1gb page
        base = pages_1gb(8) - pages_2mb(2);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_2mb(2) + pages_1gb(1));

        // 10. a 2mb page followed by some 4kb pages
        base = pages_1gb(9) - pages_2mb(1);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_2mb(1) + pages_4kb(2));

        // 11. a 1gb page followed by some 4kb pages
        base = pages_1gb(10);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_1gb(1) + pages_4kb(2));

        // 12. a 1gb page followed by some 2mb pages
        base = pages_1gb(11);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_1gb(1) + pages_2mb(2));

        // 13. some 4kb pages followed by a 2mb page, a 1gb page, then a 2mb page and a 4kb page
        base = pages_1gb(13) - pages_2mb(1) - pages_4kb(2);
        mmap.add_region(MemoryRegionType::Available, base, base + pages_4kb(2) + pages_2mb(1) + pages_1gb(1) + pages_2mb(1) + pages_4kb(1));

        let mut page_iter_4kb = PageIterator::new(&mmap, PAGE_SIZE_4KB);
        let mut page_iter_2mb = PageIterator::new(&mmap, PAGE_SIZE_2MB);
        let mut page_iter_1gb = PageIterator::new(&mmap, PAGE_SIZE_1GB);

        // 1. multiple 4kb pages in a region
        assert_eq!(page_iter_4kb.next(), Some(0));
        assert_eq!(page_iter_4kb.next(), Some(0x1000));
        
        // 2. a single 4kb page in a region
        assert_eq!(page_iter_4kb.next(), Some(0x4000));

        // 3. multiple 2mb pages in a region
        assert_eq!(page_iter_2mb.next(), Some(0x200000));
        assert_eq!(page_iter_2mb.next(), Some(0x400000));

        // 4. a single 2mb page in a region
        assert_eq!(page_iter_2mb.next(), Some(0x800000));

        // 5. multiple 1gb pages in a region
        assert_eq!(page_iter_1gb.next(), Some(0x40000000));
        assert_eq!(page_iter_1gb.next(), Some(0x80000000));
        assert_eq!(page_iter_1gb.next(), Some(0xC0000000));

        // 6. a single 1gb page in a region
        assert_eq!(page_iter_1gb.next(), Some(0x100000000));

        // 7. some 4kb pages followed by a 2mb page
        assert_eq!(page_iter_4kb.next(), Some(0x180000000 - pages_4kb(2)));
        assert_eq!(page_iter_4kb.next(), Some(0x180000000 - pages_4kb(1)));
        assert_eq!(page_iter_2mb.next(), Some(0x180000000));

        // 8. some 4kb pages followed by a 1gb page
        assert_eq!(page_iter_4kb.next(), Some(0x1C0000000 - pages_4kb(2)));
        assert_eq!(page_iter_4kb.next(), Some(0x1C0000000 - pages_4kb(1)));
        assert_eq!(page_iter_1gb.next(), Some(0x1C0000000));

        // 9. some 2mb pages followed by a 1gb page
        assert_eq!(page_iter_2mb.next(), Some(0x200000000 - pages_2mb(2)));
        assert_eq!(page_iter_2mb.next(), Some(0x200000000 - pages_2mb(1)));
        assert_eq!(page_iter_1gb.next(), Some(0x200000000));
                                            
        // 10. a 2mb page followed by some 4kb pages
        assert_eq!(page_iter_2mb.next(), Some(0x240000000 - pages_2mb(1)));
        assert_eq!(page_iter_4kb.next(), Some(0x240000000));
        assert_eq!(page_iter_4kb.next(), Some(0x240000000 + pages_4kb(1)));

        // 11. a 1gb page followed by some 4kb pages
        assert_eq!(page_iter_1gb.next(), Some(0x280000000));
        assert_eq!(page_iter_4kb.next(), Some(0x2C0000000));
        assert_eq!(page_iter_4kb.next(), Some(0x2C0000000 + pages_4kb(1)));

        // 12. a 1gb page followed by some 2mb pages
        assert_eq!(page_iter_1gb.next(), Some(0x2C0000000));
        assert_eq!(page_iter_2mb.next(), Some(0x300000000 + pages_2mb(0)));
        assert_eq!(page_iter_2mb.next(), Some(0x300000000 + pages_2mb(1)));

        // 13. some 4kb pages followed by a 2mb page, a 1gb page, then a 2mb page and a 4kb page
        assert_eq!(page_iter_4kb.next(), Some(0x340000000 - pages_2mb(1) - pages_4kb(2)));
        assert_eq!(page_iter_4kb.next(), Some(0x340000000 - pages_2mb(1) - pages_4kb(1)));
        assert_eq!(page_iter_2mb.next(), Some(0x340000000 - pages_2mb(1)));
        assert_eq!(page_iter_1gb.next(), Some(0x340000000));
        assert_eq!(page_iter_2mb.next(), Some(0x380000000));
        assert_eq!(page_iter_4kb.next(), Some(0x380000000 + pages_2mb(1) + pages_4kb(0)));

        assert_eq!(page_iter_4kb.next(), None);
        assert_eq!(page_iter_2mb.next(), None);
        assert_eq!(page_iter_1gb.next(), None);
    }
}
