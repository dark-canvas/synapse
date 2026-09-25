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
