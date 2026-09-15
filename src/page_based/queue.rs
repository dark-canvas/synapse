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

        // Construct the queue from the raw pointer instead of VirtualAddress::as_mut_reference,
        // which is hard-wired to &'static mut T. We only need the pager to outlive the queue.
        //let queue: &'a mut Self = unsafe { &mut *(virt_page.as_mut_pointer::<Self>()) };
        //let queue = virt_page.as_mut_reference::<Self>();
        let queue = unsafe { &mut *virt_page.as_mut_pointer::<Self>() };
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
        let base_addr = backing_store.as_ptr() as usize as crate::Address;
        let virt_addr = VirtualAddress(base_addr);

        // The queue page is a fixed virtual range, and the allocator may consult the pager again
        // to map the queue header and free-list nodes into that page. Allow any physical address
        // within the page and map it back to the corresponding offset within the backing store.
        mock_pager.expect_get_virtual_address().returning(move |addr| {
            let offset = addr.0 - 0x1000;
            Ok(VirtualAddress(base_addr + offset))
        });

        /*
        // the allocator would've used the rest of the page as free nodes
        let node_size = std::mem::size_of::<BigSampleItem>();
        let header_size = std::mem::size_of::<Queue::<BigSampleItem>>();

        // TODO: these assertions belong in node_allocator.rs...
        let node_offsets = [
            header_size + node_size + node_size,
            header_size + node_size,
            header_size,
        ];
        */

        {
            let queue = Queue::<BigSampleItem>::new(&mock_pager);
            assert_eq!(queue as *const _ as Address, virt_addr.0);

            assert_eq!(queue.head, None);
            assert_eq!(queue.tail, None);
            //assert_ne!(queue.allocator.free, None);
            //assert_eq!(queue.allocator.free, PhysicalAddress(0x1000 + node_offsets[0]));
        }

        // the queue reference is dropped; now we can use the pager mutably again for assertions
        mock_pager.checkpoint();
    }
}
