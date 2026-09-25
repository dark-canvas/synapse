use core::marker::PhantomData;
use crate::errors::ErrCode;
use crate::arch::x86_64::pager::VirtualAddress;
use crate::pager::Pager;

pub struct OnDemandStack<'a, T: Copy> {
    pager: &'a dyn Pager,
    base_address: VirtualAddress,
    max_items: usize,
    num_items: usize,
    _phantom: PhantomData<T>,
}
    
impl<'a, T: Copy> OnDemandStack<'a, T> {
    pub fn new(pager: &'a dyn Pager, base_address: VirtualAddress, max_items: usize) -> Self {
        Self {
            pager,
            base_address,
            max_items,
            num_items: 0,
            _phantom: PhantomData,
        }
    }

    pub fn push(&mut self, item: T) -> Result<(), ErrCode> {
        if self.num_items >= self.max_items {
            return Err(ErrCode::OutOfBounds);
        }
        let offset = core::mem::size_of::<T>() * self.num_items;
        let addr = self.base_address + offset;
        let end = addr + core::mem::size_of::<T>();
        let num_pages = ((end - addr) >> self.pager.get_page_size_log2()) + 1;
        match self.pager.ensure_mapped_range(addr, num_pages as usize) {
            Ok(_) => {
                unsafe { *(addr.0 as *mut T) = item; }
                self.num_items += 1;
                Ok(())
            },
            Err(e) => Err(e),
        }
    }

    pub fn pop(&mut self) -> Result<T, ErrCode> {
        if self.num_items == 0 {
            return Err(ErrCode::OutOfBounds);
        }
        self.num_items -= 1;
        let offset = core::mem::size_of::<T>() * self.num_items;
        let addr = self.base_address + offset;
        let item = unsafe { *(addr.0 as *mut T) };
        Ok(item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate;
    use std::boxed::Box;

    use crate::arch::x86_64::pager::VirtualAddress;
    use crate::pager::test_helpers::TestPager;
    use crate::pager::MockPager;
    use crate::Address;

    #[test]
    fn test_create() {
        let pager = MockPager::new();
        let _stack = OnDemandStack::<u32>::new(&pager, VirtualAddress(0x1000000), 10);
    }

    #[test]
    fn test_push_and_pop_lifo() {
        let mut pager = TestPager::new();
        let mut backing = Box::new([0u32; 3]);
        let base = backing.as_mut_ptr() as usize as Address;

        for index in 0..3 {
            pager.expect_ensure_mapped_range()
                .with(
                    predicate::eq(VirtualAddress(base + index * core::mem::size_of::<u32>() as Address)),
                    predicate::eq(1),
                )
                .times(1)
                .returning(|_, _| Ok(()));
        }

        let mut stack = OnDemandStack::<u32>::new(pager.get_mock(), VirtualAddress(base), 3);
        assert_eq!(stack.push(10), Ok(()));
        assert_eq!(stack.push(20), Ok(()));
        assert_eq!(stack.push(30), Ok(()));
        assert_eq!(*backing, [10, 20, 30]);

        assert_eq!(stack.pop(), Ok(30));
        assert_eq!(stack.pop(), Ok(20));
        assert_eq!(stack.pop(), Ok(10));
    }

    #[test]
    fn test_push_out_of_range() {
        let mut pager = TestPager::new();
        let mut backing = Box::new([0u32; 1]);
        let base = backing.as_mut_ptr() as usize as Address;

        pager.expect_ensure_mapped_range()
            .with(predicate::eq(VirtualAddress(base)), predicate::eq(1))
            .times(1)
            .returning(|_, _| Ok(()));

        let mut stack = OnDemandStack::<u32>::new(pager.get_mock(), VirtualAddress(base), 1);
        assert_eq!(stack.push(10), Ok(()));
        assert_eq!(stack.push(20), Err(ErrCode::OutOfBounds));
        assert_eq!(*backing, [10]);
    }

    #[test]
    fn test_pop_empty() {
        let pager = MockPager::new();
        let mut stack = OnDemandStack::<u32>::new(&pager, VirtualAddress(0x1000000), 1);

        assert_eq!(stack.pop(), Err(ErrCode::OutOfBounds));
    }

    #[test]
    fn test_mapping_failure_does_not_advance_stack() {
        let mut pager = TestPager::new();
        let mut backing = Box::new([0u32; 1]);
        let base = backing.as_mut_ptr() as usize as Address;
        let mut attempts = 0;

        pager.expect_ensure_mapped_range()
            .with(predicate::eq(VirtualAddress(base)), predicate::eq(1))
            .times(2)
            .returning(move |_, _| {
                attempts += 1;
                if attempts == 1 { Err(ErrCode::Unknown) } else { Ok(()) }
            });

        let mut stack = OnDemandStack::<u32>::new(pager.get_mock(), VirtualAddress(base), 1);
        assert_eq!(stack.push(10), Err(ErrCode::Unknown));
        assert_eq!(*backing, [0]);
        assert_eq!(stack.push(20), Ok(()));
        assert_eq!(stack.pop(), Ok(20));
    }
}
