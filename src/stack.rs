//! A generic stack implementation
//!
//! While rust already provides Vec<T> as a stack-like data structure, it relies on 
//! heap allocation, and this code exists a layer below that.
//!
//! The diagram below depicts how the different stack types grow in memory.
//! Effectively, the direction ("up" or "down") refers to the math 
//! ("increment" or "decrement") used to calculate the next address when 
//! pushing a new value onto the stack.
//!
//! ```
//!     Memory      Expand Up   Expand Down
//!  ============ ============ =============
//!   0x00000040       limit      1 <- base
//!   0x00000038                  2
//!   0x00000030                  3
//!   0x00000028                  4 <- top
//!   0x00000020
//!   0x00000018      4 <- top
//!   0x00000010      3
//!   0x00000008      2
//!   0x00000000      1 <- base    limit
//!  ```
//!
//! Note that the stack type is declared as a const generic parameter, which means that 
//! many of the brances and conditional can be optimized or compiled out entirely.

use crate::types::Address;

pub const EXPAND_UP: bool = true;
pub const EXPAND_DOWN: bool = false;

pub struct Stack<'a, T, const STACK_TYPE: bool> where T: Clone + Copy {
    size: usize,
    base: &'a mut[T],
    pointer: usize,
}

impl<'a, T: Clone + Copy, const STACK_TYPE: bool> Stack<'a, T, STACK_TYPE> {
    pub fn new(base: Address, size: usize) -> Stack<'static, T, STACK_TYPE> {
        Stack { 
            base: unsafe { core::slice::from_raw_parts_mut(base as *mut T, size) }, 
            size, 
            pointer: match(STACK_TYPE) {
                EXPAND_UP => 0,
                EXPAND_DOWN => size,
            }
        }
    }

    fn direction(&self) -> isize {
        match(STACK_TYPE) {
            EXPAND_UP => 1,
            EXPAND_DOWN => -1,
        }
    }

    pub fn base(&self) -> Address {
        (
            match(STACK_TYPE) {
                EXPAND_UP => self.base.as_ptr() as usize,
                EXPAND_DOWN => self.base.as_ptr() as usize + (self.size * core::mem::size_of::<T>()),
            }
        ) as Address
    }

    pub fn top(&self) -> Address {
        (
           match(STACK_TYPE) {
                EXPAND_UP => self.base() as usize + (self.len() * core::mem::size_of::<T>()),
                EXPAND_DOWN => self.base() as usize - (self.len() * core::mem::size_of::<T>()),
            }
        ) as Address
    }

    pub fn limit(&self) -> Address {
        self.base() + (self.direction() * (self.size * core::mem::size_of::<T>()) as isize) as Address
    }

    pub fn is_empty(&self) -> bool {
        match STACK_TYPE {
            EXPAND_UP => self.pointer == 0,
            EXPAND_DOWN => self.pointer == self.size,
        }
    }

    pub fn is_full(&self) -> bool {
        match STACK_TYPE {
            EXPAND_UP => self.pointer >= self.size,
            EXPAND_DOWN => self.pointer == 0,
        }
    }

    pub fn len(&self) -> usize {
        match STACK_TYPE {
            EXPAND_UP => self.pointer,
            EXPAND_DOWN => self.size - self.pointer,
        }
    }

    pub fn capacity(&self) -> usize {
        self.size
    }

    pub fn available(&self) -> usize {
        self.capacity() - self.len()
    }

    pub fn push(&mut self, value: T) {
        if self.is_full() {
            panic!("Stack overflow");
        }
        
        match STACK_TYPE {
            EXPAND_UP => {
                self.base[self.pointer] = value;
                self.pointer += 1;
            },
            EXPAND_DOWN => {
                self.pointer -= 1;
                self.base[self.pointer] = value;
            }
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        self.pointer -= self.direction() as usize;
        Some(self.base[self.pointer])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_stack() {
        let stack = Stack::<usize, EXPAND_UP>::new(0x1000, 10);

        assert_eq!(stack.size, 10);
        assert_eq!(stack.len(), 0);
        assert_eq!(stack.capacity(), 10);
        assert_eq!(stack.available(), 10);
        assert_eq!(stack.base(), 0x1000 as Address);
        assert_eq!(stack.top(), 0x1000 as Address);
        assert_eq!(stack.limit() as usize, 0x1000 + 10 * core::mem::size_of::<usize>());
    }

    #[test]
    fn test_expand_up_push() {
        let stack_data = [0usize; 10];
        let mut stack = Stack::<usize, EXPAND_UP>::new(stack_data.as_ptr() as Address, 10);
        stack.push(0x12345678);
        stack.push(0x9abcdef0);
        assert_eq!(stack.base[0], 0x12345678);
        assert_eq!(stack.base[1], 0x9abcdef0);
        assert_eq!(stack.size, 10);
        assert_eq!(stack.len(), 2);
        assert_eq!(stack.capacity(), 10);
        assert_eq!(stack.available(), 8);
        assert_eq!(stack.base(), stack_data.as_ptr() as Address);
        assert_eq!(stack.top(), (stack_data.as_ptr() as usize + 2 * core::mem::size_of::<usize>()) as Address);
    }

    #[test]
    fn test_expand_down_push() {
        let stack_data = [0usize; 10];
        let mut stack = Stack::<usize, EXPAND_DOWN>::new(stack_data.as_ptr() as Address, 10);
        stack.push(0x12345678);
        stack.push(0x9abcdef0);
        assert_eq!(stack.base[9], 0x12345678);
        assert_eq!(stack.size, 10);
        assert_eq!(stack.len(), 2);
        assert_eq!(stack.capacity(), 10);
        assert_eq!(stack.available(), 8);
        assert_eq!(stack.base(), (stack_data.as_ptr() as usize + 10 * core::mem::size_of::<usize>()) as Address);
        assert_eq!(stack.top(), (stack_data.as_ptr() as usize + 8 * core::mem::size_of::<usize>()) as Address);
    }
}
