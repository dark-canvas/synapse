use crate::page_based::node_allocator::NodeAllocator;
use crate::errors::ErrCode;
use crate::pager::Pager;

pub struct Queue<T: Copy> {
    head: Option<*mut LinkNode<T>>,
    tail: Option<*mut LinkNode<T>>,
    allocator: NodeAllocator<LinkNode<T>>,
}

struct LinkNode<T: Copy> {
    item: T,
    next: Option<*mut LinkNode<T>>,
}

impl<T: Copy> Queue<T> {
    pub fn new(pager: &'static dyn Pager) -> &'static mut Self {
        // The backing storage lives at a fixed virtual address for the lifetime
        // of the kernel, so this reference is permanently valid.
        let phys_page = pager.allocate_physical().unwrap();
        let virt_page = pager.get_virtual_address(phys_page).unwrap();

        let queue: &'static mut Self = virt_page.as_mut_reference::<Self>();
        queue.head = None;
        queue.tail = None;
        queue.allocator = NodeAllocator::new(pager);
        queue.allocator.use_page(phys_page, core::mem::size_of::<Self>());

        queue
    }

    // Adds to the tail...
    pub fn enqueue(&mut self, item: T) -> Result<(), ErrCode> {
        let node = self.allocator.allocate()?;
        (*node).item = item;
        (*node).next = None;
        match self.tail {
            Some(tail_node) => {
                unsafe {
                    (*tail_node).next = Some(node);
                }
            }
            None => {
                self.head = Some(node);
            }
        }
        self.tail = Some(node);
        Ok(())
    }

    // Pops from the head
    pub fn dequeue(&mut self) -> Result<T, ErrCode> {
        match self.head {
            Some(head_node) => {
                unsafe {
                    let next_node = (*head_node).next;
                    self.head = next_node;
                    if next_node.is_none() {
                        self.tail = None;
                    }
                    return Ok((*head_node).item);
                }
            }
            None => return Err(ErrCode::OutOfBounds),
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
    struct SampleItem {
        value: u32,
    }

    #[derive(Copy, Clone)]
    struct BigSampleItem {
        block: [u32; 1024],
    }

    #[test]
    fn test_create_queue() {
        let mock_pager = Box::leak(Box::new(MockPager::new()));

        mock_pager.expect_get_page_size().returning(|| 4096);
        mock_pager.expect_get_page_mask().returning(|| 4095);
        mock_pager.expect_allocate_physical()
            .times(1)
            .returning(|| Ok(PhysicalAddress(0x1000)));

        let backing_store = Box::leak(Box::new([0u8; 4096]));
        let virt_addr = VirtualAddress(backing_store.as_ptr() as usize as crate::Address);

        mock_pager.expect_get_virtual_address()
            .times(1)
            .withf(|addr| *addr == PhysicalAddress(0x1000))
            .returning(move |_| Ok(virt_addr));

        // page is 4096 bytes, and "BigSampleItem" is 1024 bytes.
        // The header takes up a few bytes of the page, so it can't fit a full 8 BigSampleItem's 
        // in the initially allocated page, but it should be able to fit 3 of them.

        // in order to free each node, it must be able to convert it to a virtual address 
        // in order to write to it (to add to the free list)
        let queue_header_size = core::mem::size_of::<Queue::<BigSampleItem>>();
        for node_addr in [ 
            queue_header_size,
            queue_header_size + 1024,
            queue_header_size + 2048,
        ] {
            mock_pager.expect_get_virtual_address()
                .times(1)
                .with(predicate::eq(PhysicalAddress(0x1000 + node_addr as Address)))
                .returning(move |_| Ok(virt_addr + node_addr));
        }

        let queue = Queue::<BigSampleItem>::new(mock_pager);

        assert_eq!(queue as *const _ as Address, virt_addr.0);
        
        // I can't do this because mock_pager is borrowed for 'static already (above)!
        //mock_pager.checkpoint();
    }
}
