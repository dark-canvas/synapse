use core::default::Default;
use crate::pager::PAGER;

pub fn new<T: Default>() -> &'static mut T {
    let pager = PAGER.borrow();
    let phys_address = pager.allocate_physical().unwrap();
    let result : &'static mut T = unsafe { &mut *(pager.get_virtual_address(phys_address).unwrap().0 as *mut T) };

    *result = T::default();
    result
}