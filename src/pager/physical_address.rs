use core::ops::{Add, AddAssign, Sub};

use crate::types::Address;


#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PhysicalAddress(pub Address);

impl Add<usize> for PhysicalAddress {
    type Output = PhysicalAddress;

    fn add(self, rhs: usize) -> Self::Output {
        PhysicalAddress(self.0 + rhs as Address)
    }
}

impl AddAssign<usize> for PhysicalAddress {
    fn add_assign(&mut self, rhs: usize) {
        self.0 += rhs as Address;
    }
}

impl Sub<usize> for PhysicalAddress {
    type Output = PhysicalAddress;

    fn sub(self, rhs: usize) -> Self::Output {
        PhysicalAddress(self.0 - rhs as Address)
    }
}

impl Sub<PhysicalAddress> for PhysicalAddress {
    type Output = usize;

    fn sub(self, rhs: PhysicalAddress) -> Self::Output {
        (self.0 - rhs.0) as usize
    }
}

impl PhysicalAddress {
    pub fn as_pointer<T>(&self) -> *const T {
        self.0 as usize as *const T
    }

    pub fn as_mut_pointer<T>(&self) -> *mut T {
        self.0 as usize as *mut T
    }
}
