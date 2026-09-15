
use core::marker::PhantomData;
use crate::arch::x86_64::pager::{PhysicalAddress, VirtualAddress};
use crate::errors::ErrCode;
use crate::pager::Pager;
use satus_struct::types::Address;
// TODO: need to determine where to use physical vs virtual addresses

/// Uses dynamically allocated pages to present an allocator of node-sized chucks.
/// The free list of nodes is stored as physical addresses so that they can be mapped into any virtual address space.
/// REVISIT: is this useful?  Probably for message queues and mutex/semaphores.
pub struct NodeAllocator<'a, T> {
    pager: &'a dyn Pager,
    free: PhysicalAddress,
    _phantom: PhantomData<T>,
}

struct FreeNode {
    next: PhysicalAddress,
}

// TODO: impl Drop for NodeAllocator... free all pages that've been allocated

impl<'a, T> NodeAllocator<'a, T> {
    pub fn new(pager: &'a dyn Pager) -> Self {
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


#[cfg(test)]
mod tests {
    use super::*;
    use std::boxed::Box;
    use std::mem::drop;
    use mockall::predicate;

    use crate::arch::x86_64::pager::{PhysicalAddress, VirtualAddress};
    use crate::pager::MockPager;
    use crate::Address;

    #[derive(Copy, Clone)]
    struct BigSampleItem {
        block: [u8; 1024],
    }

    #[test]
    fn test_create_allocator() {
        let pager = MockPager::new();
        let allocator = NodeAllocator::<u64>::new(&pager);
        // shouldn't call any methods on the pager yet
    }

    #[test]
    fn test_use_part_page() {
        let mut mock_pager = MockPager::new();
        let node_size = std::mem::size_of::<BigSampleItem>();

        let page = Box::new([0u8; 4096]);
        let base_addr = page.as_ptr() as usize as crate::Address;

        // physical address 0x1000 will map to the virtual address of "base_addr"
        // In other words, the physical address will map the buffer above.
        let phys_base : usize = 0x1000;
        mock_pager.expect_get_page_size().returning(||4096);
        mock_pager.expect_get_page_mask().returning(||4095);
        mock_pager.expect_get_virtual_address().returning(move |addr| {
            let offset = addr.0 - phys_base as u64;
            Ok(VirtualAddress(base_addr + offset))
        });        
        
        let phys_offset : usize = 128;
        let mut allocator = NodeAllocator::<BigSampleItem>::new(&mock_pager);
        allocator.use_page(PhysicalAddress(phys_base as Address), phys_offset);
        
        // the above call will use the tail 4096-128 bytes of the page as nodes.
        // the nodes are 1024 bytes long (size of BigSampleItem), which means:
        let node0_offset : usize = 128;
        let node1_offset : usize = 128+1024;
        let node2_offset : usize = 128+1024+1024;
        // there isn't enough room for another node.
        // nodes are added to the head of the "free" list which means the free list 
        // currently points to node 2
        assert_eq!(allocator.free, PhysicalAddress( (phys_base + node2_offset) as Address ));

        // and that node will point to node 1
        let ptr_size : usize = std::mem::size_of::<usize>();
        let node2_next = usize::from_le_bytes(
            page[node2_offset..node2_offset + ptr_size].try_into().unwrap());
        assert_eq!(node2_next, phys_base + node1_offset);

        // and node 1 will point to node 0
        let node1_next = usize::from_le_bytes(
            page[node1_offset..node1_offset + ptr_size].try_into().unwrap());
        assert_eq!(node1_next, phys_base + node0_offset);

        // and node 0 will point to nothing
        let node0_next = usize::from_le_bytes(
            page[node0_offset..node0_offset + ptr_size].try_into().unwrap());
        assert_eq!(node0_next, 0);
    }

    #[test]
    fn test_allocate() {
        let mut mock_pager = MockPager::new();
        let node_size = std::mem::size_of::<BigSampleItem>();

        // TODO: better way of handling multiple physical pages...
        // possibly a wrapper around the mock_pager for this...
        let page = Box::new([0u8; 4096]);
        let page2 = Box::new([0u8; 4096]);
        let base_addr = page.as_ptr() as usize as crate::Address;
        let base_addr2 = page2.as_ptr() as usize as crate::Address;

        // physical address 0x1000 will map to the virtual address of "base_addr"
        // In other words, the physical address will map the buffer above.
        let phys_base : usize = 0x1000;
        mock_pager.expect_get_page_size().returning(||4096);
        mock_pager.expect_get_page_mask().returning(||4095);
        mock_pager.expect_get_virtual_address().returning(move |addr| {
            let offset = addr.0 - phys_base as u64;
            Ok(VirtualAddress(base_addr + offset))
        });

        // after using the free list, a page must be allocated...
        mock_pager.expect_allocate_physical()
            .times(1)
            .returning(||Ok(PhysicalAddress(0x2000)));

        
        let phys_offset : usize = 128;
        let mut allocator = NodeAllocator::<BigSampleItem>::new(&mock_pager);
        allocator.use_page(PhysicalAddress(phys_base as Address), phys_offset);
        
        // the above call will use the tail 4096-128 bytes of the page as nodes.
        // the nodes are 1024 bytes long (size of BigSampleItem), which means:
        let node0 = base_addr + 128;
        let node1 = base_addr + 128+1024;
        let node2 = base_addr + 128+1024+1024;

        // first 3 nodes are allocated from the free list...
        for expected_node in [ node2, node1, node0 ] {
            let node = allocator.allocate();
            assert!(node.is_ok());
            assert_eq!(node.unwrap() as *mut BigSampleItem, expected_node as *mut BigSampleItem);
        }

        // next node is allocated from a new page
        let node = allocator.allocate();
        assert!(node.is_ok());
        // TODO: assert that this node is contained in the other physical page
    }
}