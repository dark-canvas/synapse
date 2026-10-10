use core::ops::{Add, AddAssign, Sub};

use crate::types::Address;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct VirtualAddress(pub Address);

impl Add<usize> for VirtualAddress {
    type Output = VirtualAddress;

    fn add(self, rhs: usize) -> Self::Output {
        VirtualAddress(self.0 + rhs as Address)
    }
}

impl AddAssign<usize> for VirtualAddress {
    fn add_assign(&mut self, rhs: usize) {
        self.0 += rhs as Address;
    }
}

impl Sub<usize> for VirtualAddress {
    type Output = VirtualAddress;

    fn sub(self, rhs: usize) -> Self::Output {
        VirtualAddress(self.0 - rhs as Address)
    }
}

// subtracting addresses (eg. top - bottom, or item - base) yields a size
impl Sub<VirtualAddress> for VirtualAddress {
    type Output = usize;

    fn sub(self, rhs: VirtualAddress) -> Self::Output {
        (self.0 - rhs.0) as usize
    }
}

impl VirtualAddress {
    pub fn as_pointer<T>(&self) -> *const T {
        self.0 as usize as *const T
    }

    pub fn as_mut_pointer<T>(&self) -> *mut T {
        self.0 as usize as *mut T
    }

    pub fn as_reference<T>(&self) -> &T {
        unsafe { &*self.as_pointer::<T>() }
    }

    pub fn as_mut_reference<T>(&self) -> &mut T {
        unsafe { &mut *self.as_mut_pointer::<T>() }
    }

}
