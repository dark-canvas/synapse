use core::default::Default;
use crate::arch::x86_64::pager::VirtualAddress;
use crate::Address;
use crate::pager::PAGER;

// TODO: return Result
// TODO: ensure sizeof<T> is < PAGE_SIZE
pub fn new<T: Default>() -> &'static mut T {
    let pager = PAGER.borrow();
    let phys_address = pager.allocate_physical().unwrap();
    let result : &'static mut T = unsafe { &mut *(pager.get_virtual_address(phys_address).unwrap().0 as *mut T) };

    *result = T::default();
    result
}

pub fn delete<T: Default>(page: &T) {
    let pager = PAGER.borrow();
    let page_ptr = page as *const T as Address;
    pager.free_virtual(1, VirtualAddress(page_ptr));
}