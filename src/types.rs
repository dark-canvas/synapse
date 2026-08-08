pub type Address = u64;

/*
TODO:
Make address a concrete type, and add 
    - as_mut_ptr<T>(&self) -> *mut T
    - as_ptr<T>(&self) -> *const T
    - add/sub operators
    - add/ sub assign operators
    - add comparison operators
    - add conversion to/from usize, u64, etc.

pub struct Address(u64);

impl Address {
    pub fn as_mut_ptr<T>(&self) -> *mut T {
        *self as *mut T
    }

    pub fn as_ptr<T>(&self) -> *const T {
        *self as *const T
    }
}
*/