
use core::marker::PhantomData;
use crate::arch::x86_64::pager::{PhysicalAddress, VirtualAddress};
use crate::errors::ErrCode;
use crate::pager::get_pager;
use satus_struct::types::Address;
// TODO: need to determine where to use physical vs virtual addresses

/// Uses dynamically allocated pages to present an allocator of node-sized chucks.
/// The free list of nodes is stored as physical addresses so that they can be mapped into any virtual address space.
/// REVISIT: is this useful?  Probably for message queues and mutex/semaphores.
pub struct NodeAllocator<T> {
    free: PhysicalAddress,
    _phantom: PhantomData<T>,
}

struct FreeNode {
    next: PhysicalAddress,
}

// TODO: impl Drop for NodeAllocator... free all pages that've been allocated

impl<T> NodeAllocator<T> {
    pub fn new() -> Self {
        assert!(core::mem::size_of::<T>() >= core::mem::size_of::<FreeNode>(), "NodeAllocator<T> requires T to be at least as large as FreeNode");
        Self {
            free: PhysicalAddress(0),
            _phantom: PhantomData,
        }
        // if init_from_self then call init_from_self(), but that assumes Self is already places within the page 
        // which it isn't...
    }

    pub fn free(&mut self, node: &T) -> Result<(), ErrCode> {
        let phys = get_pager().get_physical_address(VirtualAddress(node as *const T as Address))?;
        self.free_phys(phys)
    }

    fn free_phys(&mut self, node: PhysicalAddress) -> Result<(), ErrCode> {
        // convert node to a FreeNode, and store the current free list head in its next pointer
        unsafe {
            let mut free_node = node.as_mut_pointer::<FreeNode>();
            (*free_node).next = self.free;
            self.free = PhysicalAddress(free_node as *const FreeNode as Address);
        }
        Ok(())
    }

    /// This NodeAllocator could exist anywhere within a page (there could be an abritrary header before it).
    /// By calculating the position of the NodeAllocator within the page (and assuming the rest of the page is free 
    /// for us to use) then chop the rest of the page into nodes of type T and add them to the free list.
    pub fn init_from_self(&mut self) -> Result<(), ErrCode> {
        let page_size = get_pager().get_page_size();
        let page_mask = get_pager().get_page_mask();

        let node_phys = PhysicalAddress(self as *const Self as Address);
        let page_start = PhysicalAddress(node_phys.0 & !page_mask);

        let node_virt = get_pager().get_virtual_address(node_phys)?;

        let mut offset = (node_phys - page_start) + core::mem::size_of::<Self>();
        while offset + core::mem::size_of::<T>() <= page_size {
            let node_phys = page_start + offset;
            let node_virt = get_pager().get_virtual_address(node_phys)?;
            self.free(
                unsafe {
                    &*(node_virt.as_mut_pointer::<T>())
                }
            )?;
            offset += core::mem::size_of::<T>();
        }
        Ok(())
    }

    pub fn allocate(&mut self) -> Result<&mut T, ErrCode> {
        if self.free.0 == 0 {
            // allocate a new node
            let phys = get_pager().allocate_physical()?;
            let virt = get_pager().get_virtual_address(phys)?;

            let new_node = unsafe { &mut *(virt.0 as *mut T) };

            let mut offset = core::mem::size_of::<T>();
            while offset < get_pager().get_page_size() {
                let next_phys = phys + offset;
                //let next_virt = get_pager().get_virtual_address(next_phys)?;
                self.free_phys(next_phys)?;
                offset += core::mem::size_of::<T>();
            }

            Ok(new_node)
        } else {
            // pop a node from the free list
            let node_phys = self.free;
            let node_virt = get_pager().get_virtual_address(node_phys)?;
            unsafe {
                let free_node = node_virt.0 as *mut FreeNode;
                self.free = (*free_node).next;
                Ok(&mut *(node_virt.0 as *mut T))
            }
        }
    }
}