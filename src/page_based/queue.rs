use crate::page_based::node_allocator::NodeAllocator;
use crate::errors::ErrCode;
use crate::pager::Pager;

pub struct Queue<'a, T: Copy> {
    head: Option<*mut LinkNode<T>>,
    tail: Option<*mut LinkNode<T>>,
    allocator: NodeAllocator<'a, LinkNode<T>>,
}

struct LinkNode<T: Copy> {
    item: T,
    next: Option<*mut LinkNode<T>>,
}

impl<'a, T: Copy> Queue<'a, T> {
    pub fn new(pager: &'a dyn Pager) -> &'a mut Self {
        // The backing storage lives in a virtual page provided by the pager and must
        // outlive the returned queue reference. Tie the queue lifetime to the pager's.
        let phys_page = pager.allocate_physical().unwrap();
        let virt_page = pager.get_virtual_address(phys_page).unwrap();

        let queue: &'a mut Self = virt_page.as_mut_reference::<Self>();
        queue.head = None;
        queue.tail = None;
        queue.allocator = NodeAllocator::new(pager);
        queue.allocator.use_page(phys_page, core::mem::size_of::<Self>()).unwrap();

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
        // The API previously required a 'static pager; change allows the pager to simply
        // outlive the queue. Use a local mock pager and drop the queue before mutably
        // using the pager (e.g., calling checkpoint).
        let mut mock_pager = MockPager::new();

        mock_pager.expect_get_page_size().returning(|| 4096);
        mock_pager.expect_get_page_mask().returning(|| 4095);
        mock_pager.expect_allocate_physical()
            .times(1)
            .returning(|| Ok(PhysicalAddress(0x1000)));

        let backing_store = Box::new([0u8; 4096]);
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

        // create the queue borrowing from mock_pager
        let queue = Queue::<BigSampleItem>::new(&mock_pager);

        assert_eq!(queue as *const _ as Address, virt_addr.0);
        
        // drop the queue so the pager can be used mutably afterwards
        drop(queue);

        // now it's possible to mutably borrow the mock pager to checkpoint or assert expectations
        mock_pager.checkpoint();
    }
}
