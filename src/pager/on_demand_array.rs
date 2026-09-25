use core::marker::PhantomData;
use crate::errors::ErrCode;
use crate::arch::x86_64::pager::VirtualAddress;
use crate::pager::Pager;

pub struct OnDemandArray<'a, T> {
    pager: &'a dyn Pager, 
    base_address: VirtualAddress,
    num_items: usize,
    _phantom: PhantomData<T>,
}

impl<'a, T> OnDemandArray<'a, T> {
    pub fn new(pager: &'a dyn Pager, base_address: VirtualAddress, num_items: usize) -> Self {
        Self {
            pager,
            base_address,
            num_items,
            _phantom: PhantomData,
        }
    }

    pub fn get_mut(&self, i: usize) -> Result<&'static mut T, ErrCode> {
        if i >= self.num_items {
            return Err(ErrCode::OutOfBounds);
        }
        let offset = core::mem::size_of::<T>() * i;
        let base = self.base_address + offset;
        let end = base + core::mem::size_of::<T>();
        let num_pages = ((end - base) >> self.pager.get_page_size_log2()) + 1;
        match self.pager.ensure_mapped_range(base, num_pages as usize) {
            Ok(_) => Ok(unsafe { &mut *(base.0 as *mut T) }),
            Err(e) => Err(e),
        }
    }

    pub fn get(&self, i: usize) -> Result<&'static T, ErrCode> {
        match self.get_mut(i) {
            Ok(r) => Ok(r),
            Err(e) => Err(e),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate;

    use crate::arch::x86_64::pager::{PhysicalAddress, VirtualAddress};
    use crate::pager::MockPager;
    use crate::pager::test_helpers::TestPager;
    use crate::pager::test_helpers::PhysicalVirtualMapping;
    use crate::Address;

    #[derive(Debug, PartialEq)]
    struct HalfPage {
        data: [u8; 2048],
    }

    #[test]
    fn test_create() {
        let pager = MockPager::new();

        // shouldn't invoke the pager at all...
        let array = OnDemandArray::<u64>::new(&pager, VirtualAddress(0x1000000), 10);
    }

    #[test]
    fn test_get_first() {
        let mut pager = TestPager::new();

        pager.expect_ensure_mapped_range()
            .with(
                predicate::eq(VirtualAddress(0x1000000)), 
                predicate::eq(1)
            )
            .times(1)
            .returning(|_,_| Ok(()) );

        let array = OnDemandArray::<HalfPage>::new(pager.get_mock(), VirtualAddress(0x1000000), 10);
        let result = array.get(0);
        assert!(result.is_ok());
        assert_eq!( result.unwrap() as *const HalfPage, 0x1000000 as *const HalfPage)
    }


    #[test]
    fn test_get_inner() {
        let mut pager = TestPager::new();

        // 2 "HalfPage" objects fit per page, so..
        // Index          | 0 |      1 |     2 |      3 |      4 |      5 |      6
        // Page (base + ) | 0 | 0x0800 | 0x1000| 0x1800 | 0x2800 | 0x2800 | 0x3800 
        let base = 0x1000000;
        let offset = 0x3000;
        let element_size = 2048;

        pager.expect_ensure_mapped_range()
            .with(
                predicate::eq(VirtualAddress(base + offset)), 
                predicate::eq(1)
            )
            .times(1)
            .returning(|_,_| Ok(()) );

        let array = OnDemandArray::<HalfPage>::new(pager.get_mock(), VirtualAddress(base), 10);
        let result = array.get(6);
        assert!(result.is_ok());
        assert_eq!( result.unwrap() as *const HalfPage, (base + offset) as *const HalfPage);
    }

    #[test]
    fn test_get_middle() {
        let mut pager = TestPager::new();

        // 2 "HalfPage" objects fit per page, so..
        // Index          | 0 |      1 |     2 |      3 |      4 |      5 |      6
        // Page (base + ) | 0 | 0x0800 | 0x1000| 0x1800 | 0x2800 | 0x2800 | 0x3800 
        let base = 0x1000000;
        let offset = 0x1800;

        pager.expect_ensure_mapped_range()
            .with(
                predicate::eq(VirtualAddress(base + offset)), 
                predicate::eq(1)
            )
            .times(1)
            .returning(|_,_| Ok(()) );

        let array = OnDemandArray::<HalfPage>::new(pager.get_mock(), VirtualAddress(base), 10);
        let result = array.get(3);
        assert!(result.is_ok());
        assert_eq!( result.unwrap() as *const HalfPage, (base + offset) as *const HalfPage);
    }

    #[test]
    fn test_get_out_of_range() {
        let pager = MockPager::new();

        // shouldn't invoke the pager at all...
        let array = OnDemandArray::<u64>::new(&pager, VirtualAddress(0x1000000), 10);
        let result = array.get(10);
        assert_eq!(result.is_ok(), false);
        assert_eq!(result, Err(ErrCode::OutOfBounds));
    }

    #[test]
    fn test_couldnt_map() {
        let mut pager = TestPager::new();
        let base = 0x1000000;
        let offset = 0x1800;

        pager.expect_ensure_mapped_range()
            .with(
                predicate::eq(VirtualAddress(base + offset)), 
                predicate::eq(1)
            )
            .times(1)
            .returning(|_,_| Err(ErrCode::Unknown) );

        let array = OnDemandArray::<HalfPage>::new(pager.get_mock(), VirtualAddress(base), 10);
        let result = array.get(3);
        assert_eq!(result.is_ok(), false);
        assert_eq!(result, Err(ErrCode::Unknown));
    }
}