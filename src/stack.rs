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
//!
//! While the underlying memory layout of the expand up and expand down stacks differs, the 
//! functions which accept and provide indices are meant to behave in the same way.  The 0th 
//! index will refer to the first item pushed onto the stack, with increasing indices getting 
//! closer to the most recently pushed element (len()-1 being the last pushed element)

use crate::types::Address;
use core::ops::Index; 

pub const EXPAND_UP: bool = true;
pub const EXPAND_DOWN: bool = false;

pub trait SimpleStack<T> {
    fn push(&mut self, value: T);
    fn pop(&mut self) -> Option<T>;
}

pub struct Stack<'a, T, const STACK_TYPE: bool> where T: Clone + Copy + PartialEq {
    size: usize,
    base: &'a mut[T],
    pointer: usize,
}

pub struct StackIterator<'a, T, const STACK_TYPE: bool> where T: Clone + Copy + PartialEq {
    stack: &'a Stack<'a, T, STACK_TYPE>,
    num: isize,
    direction: isize,
}

impl<'a, T, const STACK_TYPE: bool> Iterator for StackIterator<'a, T, STACK_TYPE> 
where T: Clone + Copy + PartialEq {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.stack.get(self.num as usize);
        self.num += self.direction;
        result
    }
}

impl<'a, T, const STACK_TYPE: bool> StackIterator<'a, T, STACK_TYPE> 
where T: Clone + Copy + PartialEq {
    pub fn rev(mut self) -> Self {
        self.direction = -1;
        self.num = self.stack.len() as isize - 1;
        self
    }

    pub fn get_index(&self) -> usize {
        self.num as usize
    }
}

#[allow(dead_code)]
impl<'a, T: Clone + Copy + PartialEq, const STACK_TYPE: bool> Stack<'a, T, STACK_TYPE> {
    pub fn new(base: Address, size: usize) -> Stack<'static, T, STACK_TYPE> {
        Stack { 
            base: unsafe { core::slice::from_raw_parts_mut(base as *mut T, size) }, 
            size, 
            pointer: match STACK_TYPE {
                EXPAND_UP => 0,
                EXPAND_DOWN => size,
            }
        }
    }

    fn direction(&self) -> isize {
        match STACK_TYPE {
            EXPAND_UP => 1,
            EXPAND_DOWN => -1,
        }
    }

    // It's assume that the index has already been validated
    fn logical_to_array_index(&self, index: usize) -> usize {
        match STACK_TYPE {
            EXPAND_UP => index,
            EXPAND_DOWN => self.size - 1 - index,
        }
    }

    pub fn base(&self) -> Address {
        (
            match STACK_TYPE {
                EXPAND_UP => self.base.as_ptr() as usize,
                EXPAND_DOWN => self.base.as_ptr() as usize + (self.size * core::mem::size_of::<T>()),
            }
        ) as Address
    }

    pub fn top(&self) -> Address {
        (
           match STACK_TYPE {
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

    /// Expensive; performs a linear search
    /// Could be made faster if the data was kept sorted, but given that this is (currently) only 
    /// used to construct the page stacks initially, it's probably not horrible.
    pub fn find(&self, value: T) -> Option<usize> {
        let (start, end) = match STACK_TYPE {
            EXPAND_UP => (0, self.pointer),
            EXPAND_DOWN => (self.pointer, self.size)
        };

        for i in start..end {
            if self.base[i] == value {
                return match STACK_TYPE {
                    EXPAND_UP => Some(i),
                    EXPAND_DOWN => Some(self.size-1 - i),
                }
            }
        }
        None
    }

    pub fn remove_index(&mut self, index: usize) -> Option<T> {
        if index < self.len() {
            let index = self.logical_to_array_index(index);
            // swap the last item with this one
            let result = self.base[index];
            if let Some(last) = self.pop() {
                self.base[index] = last;
                return Some(result);
            }
        }
        None
    }

    /// Get an element from the stack, whereby the 0th element would be the first 
    /// element pushed on the stack, and the indicies increment towards the top 
    /// of the stack (i.e., returning the most recently pushed item onto the stack via the 
    /// len()-1 index).
    pub fn get(&self, index: usize) -> Option<T> {
        if index >= self.len() {
            return None
        }

        return Some(self.get_unchecked(index));
    }

    /// Same as get() but without any assertions on whether the index is valid
    pub fn get_unchecked(&self, index: usize) -> T {
        match STACK_TYPE {
            EXPAND_UP => self.base[index],
            EXPAND_DOWN => self.base[self.size-1-index]
        }
    }

    // swap which accepts iterators?

    // Swap the elements at the logical indices provided
    pub fn swap(&mut self, index1: usize, index2: usize) -> bool {
        if index1 < self.len() && index2 < self.len() {
            let index1 = self.logical_to_array_index(index1);
            let index2 = self.logical_to_array_index(index2);

            let temp = self.base[index1];
            self.base[index1] = self.base[index2];
            self.base[index2] = temp;
            return true;
        }
        false
    }

    // Same as swap() but accepts absolute/array indices instead
    pub fn swap_absolute(&mut self, index1: usize, index2: usize) -> bool {
        if index1 < self.len() && index2 < self.len() {
            let temp = self.base[index1];
            self.base[index1] = self.base[index2];
            self.base[index2] = temp;
            return true;
        }
        false
    }

    /// Truncate the list at `new_length` elements
    pub fn truncate(&mut self, new_length: usize) -> usize {
        if self.len() >= new_length {
            match STACK_TYPE {
                EXPAND_UP => self.pointer = new_length,
                EXPAND_DOWN => self.pointer = self.size - new_length,
            }
        }
        self.len()
    }

    /// Returns an iterator that iterates the items on the stack in the order 
    /// which they were pushed onto the stack
    pub fn iter(&self) -> StackIterator<T, STACK_TYPE> {
        StackIterator { 
            stack: self,
            num: 0,
            direction: 1,
        }
    }

    /// Returns an iterator that iterates the items on the stack in the reverse 
    /// order that they were added (i.e., as if the stack were continually popped)
    pub fn reverse_iter(&self) -> StackIterator<T, STACK_TYPE> {
        self.iter().rev()
    }
}

impl<'a, T: Clone + Copy + PartialEq, const STACK_TYPE: bool> SimpleStack<T> for  Stack<'a, T, STACK_TYPE> {
    fn push(&mut self, value: T) {
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

    fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        self.pointer -= self.direction() as usize;
        Some(self.base[self.pointer])
    }
}

impl<'a, T: Clone + Copy + PartialEq, const STACK_TYPE: bool> Index<usize> for  Stack<'a, T, STACK_TYPE> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        &self.base[index]
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
        stack.push(0xffffffff);
        assert_eq!(stack.base[0], 0x12345678);
        assert_eq!(stack.base[1], 0x9abcdef0);
        assert_eq!(stack.base[2], 0xffffffff);
        assert_eq!(stack.get(0), Some(0x12345678));
        assert_eq!(stack.get(1), Some(0x9abcdef0));
        assert_eq!(stack.get(2), Some(0xffffffff));
        assert_eq!(stack.get(3), None);
        assert_eq!(stack.find(0x9abcdef0), Some(1));
        assert_eq!(stack.find(0xffffffff), Some(2));
        assert_eq!(stack.find(5150), None);
        assert_eq!(stack.size, 10);
        assert_eq!(stack.len(), 3);
        assert_eq!(stack.capacity(), 10);
        assert_eq!(stack.available(), 7);
        assert_eq!(stack.base(), stack_data.as_ptr() as Address);
        assert_eq!(stack.top(), (stack_data.as_ptr() as usize + 3 * core::mem::size_of::<usize>()) as Address);
    }

    #[test]
    fn test_expand_down_push() {
        let stack_data = [0usize; 10];
        let mut stack = Stack::<usize, EXPAND_DOWN>::new(stack_data.as_ptr() as Address, 10);
        stack.push(0x12345678);
        stack.push(0x9abcdef0);
        stack.push(0xffffffff);
        assert_eq!(stack.base[9], 0x12345678);
        assert_eq!(stack.base[8], 0x9abcdef0);
        assert_eq!(stack.base[7], 0xffffffff);
        assert_eq!(stack.get(0), Some(0x12345678));
        assert_eq!(stack.get(1), Some(0x9abcdef0));
        assert_eq!(stack.get(2), Some(0xffffffff));
        assert_eq!(stack.get(3), None);
        assert_eq!(stack.find(0x9abcdef0), Some(1));
        assert_eq!(stack.find(0xffffffff), Some(2));
        assert_eq!(stack.find(5150), None);
        assert_eq!(stack.size, 10);
        assert_eq!(stack.len(), 3);
        assert_eq!(stack.capacity(), 10);
        assert_eq!(stack.available(), 7);
        assert_eq!(stack.base(), (stack_data.as_ptr() as usize + 10 * core::mem::size_of::<usize>()) as Address);
        assert_eq!(stack.top(), (stack_data.as_ptr() as usize + 7 * core::mem::size_of::<usize>()) as Address);
    }

    #[test]
    fn test_iterate() {
        let expand_up_stack_data = [0usize; 10];
        let expand_dn_stack_data = [0usize; 10];

        let mut expand_up_stack = Stack::<usize, EXPAND_UP>::new(expand_up_stack_data.as_ptr() as Address, 10);
        let mut expand_dn_stack = Stack::<usize, EXPAND_DOWN>::new(expand_dn_stack_data.as_ptr() as Address, 10);

        expand_up_stack.push(0x12345678);
        expand_up_stack.push(0x9abcdef0);
        expand_up_stack.push(0xffffffff);

        expand_dn_stack.push(0x12345678);
        expand_dn_stack.push(0x9abcdef0);
        expand_dn_stack.push(0xffffffff);

        let mut iter_up = expand_up_stack.iter();
        let mut iter_dn = expand_dn_stack.iter();

        assert_eq!(iter_up.next(), Some(0x12345678));
        assert_eq!(iter_dn.next(), Some(0x12345678));
        assert_eq!(iter_up.next(), Some(0x9abcdef0));
        assert_eq!(iter_dn.next(), Some(0x9abcdef0));
        assert_eq!(iter_up.next(), Some(0xffffffff));
        assert_eq!(iter_dn.next(), Some(0xffffffff));
        assert_eq!(iter_up.next(), None);
        assert_eq!(iter_dn.next(), None);
    }

    #[test]
    fn test_reverse_iterate() {
        let expand_up_stack_data = [0usize; 10];
        let expand_dn_stack_data = [0usize; 10];

        let mut expand_up_stack = Stack::<usize, EXPAND_UP>::new(expand_up_stack_data.as_ptr() as Address, 10);
        let mut expand_dn_stack = Stack::<usize, EXPAND_DOWN>::new(expand_dn_stack_data.as_ptr() as Address, 10);

        expand_up_stack.push(0x12345678);
        expand_up_stack.push(0x9abcdef0);
        expand_up_stack.push(0xffffffff);

        expand_dn_stack.push(0x12345678);
        expand_dn_stack.push(0x9abcdef0);
        expand_dn_stack.push(0xffffffff);

        let mut iter_up = expand_up_stack.reverse_iter();
        let mut iter_dn = expand_dn_stack.reverse_iter();

        assert_eq!(iter_up.next(), Some(0xffffffff));
        assert_eq!(iter_dn.next(), Some(0xffffffff));
        assert_eq!(iter_up.next(), Some(0x9abcdef0));
        assert_eq!(iter_dn.next(), Some(0x9abcdef0));
        assert_eq!(iter_up.next(), Some(0x12345678));
        assert_eq!(iter_dn.next(), Some(0x12345678));
        assert_eq!(iter_up.next(), None);
        assert_eq!(iter_dn.next(), None);
    }
}
