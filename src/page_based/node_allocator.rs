
use core::marker::PhantomData;
use crate::arch::x86_64::pager::{PhysicalAddress, VirtualAddress};
use crate::errors::ErrCode;
use crate::pager::Pager;
use satus_struct::types::Address;
// TODO: need to determine where to use physical vs virtual addresses

/// Uses dynamically allocated pages to present an allocator of node-sized chucks.
/// The free list of nodes is stored as physical addresses so that they can be mapped into any virtual address space.
/// REVISIT: is this useful?  Probably for message queues and mutex/semaphores.
pub struct NodeAllocator<T> {
    pager: &'static dyn Pager,
    free: PhysicalAddress,
    _phantom: PhantomData<T>,
}

struct FreeNode {
    next: PhysicalAddress,
}

// TODO: impl Drop for NodeAllocator... free all pages that've been allocated

impl<T> NodeAllocator<T> {
    pub fn new(pager: &'static dyn Pager) -> Self {
        assert!(core::mem::size_of::<T>() >= core::mem::size_of::<FreeNode>(), "NodeAllocator<T> requires T to be at least as large as FreeNode");
        Self {
            pager,
            free: PhysicalAddress(0),
            _phantom: PhantomData,
        }
        // if init_from_self then call init_from_self(), but that assumes Self is already places within the page 
        // which it isn't...
    }

    pub fn free(&mut self, node: &T) -> Result<(), ErrCode> {
        let phys = self.pager.get_physical_address(VirtualAddress(node as *const T as Address))?;
        self.free_phys(phys)
    }

    // Self is a virtual address, but everything else is a physical address and must be converted to a 
    // virtual address before it can be written to
    fn free_phys(&mut self, phys_node: PhysicalAddress) -> Result<(), ErrCode> {
        let virt_node = self.pager.get_virtual_address(phys_node).unwrap();
        // convert node to a FreeNode, and store the current free list head in its next pointer
        unsafe {
            let mut free_node = virt_node.as_mut_reference::<FreeNode>();
            free_node.next = self.free;
            self.free = phys_node;
        }
        Ok(())
    }

    pub fn use_page(&mut self, page: PhysicalAddress, offset: usize) -> Result<(), ErrCode> {
        let page_size = self.pager.get_page_size();
        let page_mask = self.pager.get_page_mask();

        let page_start = PhysicalAddress(page.0 & !page_mask);
        let node_size = core::mem::size_of::<T>(); 
        let mut offset = offset;

        while offset + node_size <= page_size {
            self.free_phys(page_start + offset);
            offset += node_size;
        }
        Ok(())
    }

    pub fn allocate(&mut self) -> Result<&mut T, ErrCode> {
        if self.free.0 == 0 {
            // allocate a new node
            let phys = self.pager.allocate_physical()?;
            let virt = self.pager.get_virtual_address(phys)?;

            let new_node = unsafe { &mut *(virt.0 as *mut T) };

            let mut offset = core::mem::size_of::<T>();
            while offset < self.pager.get_page_size() {
                let next_phys = phys + offset;
                //let next_virt = self.pager.get_virtual_address(next_phys)?;
                self.free_phys(next_phys)?;
                offset += core::mem::size_of::<T>();
            }

            Ok(new_node)
        } else {
            // pop a node from the free list
            let node_phys = self.free;
            let node_virt = self.pager.get_virtual_address(node_phys)?;
            unsafe {
                let free_node = node_virt.0 as *mut FreeNode;
                self.free = (*free_node).next;
                Ok(&mut *(node_virt.0 as *mut T))
            }
        }
    }
}