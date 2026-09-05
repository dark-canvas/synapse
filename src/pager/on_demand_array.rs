use core::marker::PhantomData;
use crate::errors::ErrCode;
use crate::arch::x86_64::pager::VirtualAddress;
use super::get_pager;

pub struct OnDemandArray<T> {
    base_address: VirtualAddress,
    num_items: usize,
    _phantom: PhantomData<T>,
}

impl<T> OnDemandArray<T> {
    pub fn new(base_address: VirtualAddress, num_items: usize) -> Self {
        Self {
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
        let num_pages = ((end - base) >> get_pager().get_page_size_log2()) + 1;
        match get_pager().ensure_mapped_range(base, num_pages as usize) {
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