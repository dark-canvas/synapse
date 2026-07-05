//! page_base_list.rs - [ Singly-linked list ]
//! © 2006 neuraldk
//! 
//! Implements a singly-linked list in a nostd environment by using only page-based allocations.
//!
//! While this complicates the list in several ways, which will be discussed later, it allows 
//! for lists to exist prior to a byte allocator and also allows for list elements to be contained 
//! in separate pages and therefore mappable (or hideable) in different virtual memory spaces.
//! 
//! The basic structure of a page-based-list is:
//!
//! ```
//! PageBasedList                Element 1           Element 2
//! |------------|       /----> |---------|   /---> |---------|
//! | num_items  |      /       | next    |--/      | next    | -> 0
//! | head       | ----/        |---------|         |---------|
//! | free       | --\          | element |         | element |
//! |------------|    \         |---------|         |---------|
//!                    \        
//!                     \     free 1           free 2           free 3
//!                      \-> |------|   /---> |------|   /---> |------|
//!                          | next |--/      | next |--/      | next | -> 0
//!                          |------|         |------|         |------|
//! ```
//!
//! In the above example, the PageBaseList is a structure allocated on the stack, and all other elements are 
//! contained in pages allocated by the pager.
//!
//! The first few bytes make up the next pointer, and the rest of the page contains the element, as provided 
//! by the user of the API.
//!
//! In the above example, there is only 1 element per page and so when an element is freed it could just be 
//! returned to the pager.  However, the page-based-list has the possibility (if instructed) to try to fit 
//! multiple elements into a page, if they can fit.  In that case, each element in the head and free lists could 
//! actually be a *part* of a page.
//!
//! To illustrate this, this if a page of 4096 bytes, and an element of size 2000 bytes.  In that case, a page 
//! could hold 2 elements.  When a page is allocated for the purpose of holding an element, the page is split in 
//! 2 and one half of the page is added to the head with the element emdedded into it.  The remaining part of the 
//! page isn't yet used and is placed in the free list, to be used the next time an element is added to the list.

use crate::pager::PAGER;
use crate::Address;
use crate::errors::ErrCode;
use core::mem;
use core::ptr;

pub struct PageBasedList<T> {
    num_items: usize,
    head: Option<Address>,
    free: Option<Address>,
    _phantom: core::marker::PhantomData<T>,
}

pub struct PageBasedIterator<'a, T> {
    current: Option<Address>,
    previous: Option<Address>,
    _phantom: core::marker::PhantomData<&'a T>,
}

impl<T> PageBasedList<T> {
    pub fn new() -> Result<Self, ErrCode> {
        let pager = PAGER.borrow();
        let page_size = pager.get_page_size();
        let node_size = mem::size_of::<Address>() + mem::size_of::<T>();
        
        assert!(
            page_size >= node_size,
            "Page size must be at least the size of T and a next pointer"
        );

        Ok(PageBasedList {
            num_items: 0,
            head: None,
            free: None,
            _phantom: core::marker::PhantomData,
        })
    }

    pub fn add(&mut self, value: T) -> Result<(), ErrCode> {
        let pager = PAGER.borrow();
        let page_size = pager.get_page_size();
        let node_size = mem::size_of::<Address>() + mem::size_of::<T>();

        let node_addr = if let Some(free_addr) = self.free {
            self.free = self.pop_next(free_addr)?;
            free_addr
        } else {
            let phys = pager.allocate_physical()?;
            let virt = pager.get_virtual_address(phys)?;
            
            let remaining = page_size - node_size;
            let num_additional = remaining / node_size;
            for i in (1..=num_additional).rev() {
                let free_addr = virt.0 + (i * node_size) as u64;
                self.push_to_free(free_addr)?;
            }
            
            virt.0
        };

        unsafe {
            let next_ptr = node_addr as *mut Address;
            *next_ptr = self.head.unwrap_or(0);
            
            let data_ptr = (node_addr + mem::size_of::<Address>() as u64) as *mut T;
            ptr::write(data_ptr, value);
        }

        self.head = Some(node_addr);
        self.num_items += 1;

        Ok(())
    }

    pub fn head(&self) -> Option<&T> {
        self.head.map(|addr| unsafe {
            let data_ptr = (addr + mem::size_of::<Address>() as u64) as *const T;
            &*data_ptr
        })
    }

    pub fn head_ptr(&self) -> Option<*mut T> {
        self.head.map(|addr| {
            (addr + mem::size_of::<Address>() as u64) as *mut T
        })
    }

    pub fn iter(&self) -> PageBasedIterator<'_, T> {
        PageBasedIterator {
            current: self.head,
            previous: None,
            _phantom: core::marker::PhantomData,
        }
    }

    pub fn remove(&mut self, loc: &PageBasedIterator<T>) -> Result<(), ErrCode> {
        let current = match loc.current {
            Some(addr) => addr,
            None => return Err(ErrCode::InvalidParameter),
        };

        if let Some(prev_addr) = loc.previous {
            unsafe {
                let prev_next_ptr = prev_addr as *mut Address;
                let current_next_ptr = current as *const Address;
                *prev_next_ptr = *current_next_ptr;
            }
        } else {
            unsafe {
                let current_next_ptr = current as *const Address;
                self.head = if *current_next_ptr == 0 {
                    None
                } else {
                    Some(*current_next_ptr)
                };
            }
        }

        self.push_to_free(current)?;
        self.num_items = self.num_items.saturating_sub(1);

        Ok(())
    }

    fn push_to_free(&mut self, addr: Address) -> Result<(), ErrCode> {
        unsafe {
            let next_ptr = addr as *mut Address;
            *next_ptr = self.free.unwrap_or(0);
        }
        self.free = Some(addr);
        Ok(())
    }

    fn pop_next(&self, addr: Address) -> Result<Option<Address>, ErrCode> {
        unsafe {
            let next_ptr = addr as *const Address;
            let next = *next_ptr;
            Ok(if next == 0 { None } else { Some(next) })
        }
    }
}

impl<'a, T> PageBasedIterator<'a, T> {
    pub fn next(&mut self) -> Result<Option<&'a T>, ErrCode> {
        match self.current {
            None => Ok(None),
            Some(addr) => {
                let data = unsafe {
                    let data_ptr =
                        (addr + mem::size_of::<Address>() as u64) as *const T;
                    &*data_ptr
                };

                self.previous = Some(addr);
                self.current = unsafe {
                    let next_ptr = addr as *const Address;
                    let next = *next_ptr;
                    if next == 0 { None } else { Some(next) }
                };

                Ok(Some(data))
            }
        }
    }
}