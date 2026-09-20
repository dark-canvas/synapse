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
    next: *mut LinkNode<T>,  // Option consumes an extra word
}

impl<'a, T: Copy> Queue<'a, T> {
    const NULL_NODE : *mut LinkNode<T> = 0 as *mut LinkNode<T>;

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
        (*node).next = Self::NULL_NODE;
        match self.tail {
            Some(tail_node) => {
                unsafe {
                    (*tail_node).next = node;
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
                    self.head = Some(next_node);
                    if next_node == Self::NULL_NODE {
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
    use std::cmp::Ordering;

    use crate::arch::x86_64::pager::{PhysicalAddress, VirtualAddress};
    use crate::pager::MockPager;
    use crate::pager::test_helpers::TestPager;
    use crate::pager::test_helpers::PhysicalVirtualMapping;
    use crate::Address;

    #[derive(Copy, Clone)]
    struct SampleItem {
        value: u32,
    }

    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    struct BigSampleItem {
        block: [u32; 254], // 1016 bytes, therefore 1024 bytes when in LinkNode
    }

    impl BigSampleItem {
        pub fn new(value: u32) -> Self {
            let mut result = BigSampleItem{
                block: [0; 254],
            };
            result.block[0] = value;
            result
        }
    }

    impl PartialOrd for BigSampleItem {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for BigSampleItem {
        fn cmp(&self, other: &Self) -> Ordering {
            // Compare by age first, then by name
            self.block[0].cmp(&other.block[0])
        }
    }

    // TODO: add framework to allow queuing of nodes from a list of integers (ids)
    // node objects are traits of any variables sizes which accept set_id(id) and 
    // can response get_id(): u64.
    // The tests can then have simple assertions:
    // for id in [ 0 1 2 3 4 5] {
    //   queue.enqueue(node::new(id))
    // }
    // and...
    // for id in [ 0 1 2 3 4 5] {
    //   let node = queue.dequeue().unwrap();
    //   assert!(node.get_id(), id);
    // }
    // framework needs to create physical<->virtual memory mappings to hold 
    // the various nodes.

    #[test]
    fn test_create_queue() {
        let mut mock_pager = TestPager::new();

        let phys_addr = PhysicalAddress(0x1000);
        mock_pager.allow_allocate_physical(phys_addr);

        let backing_store = Box::new([0u8; 4096]);
        let base_addr = backing_store.as_ptr() as usize as crate::Address;
        let virt_addr = VirtualAddress(base_addr);

        mock_pager.add_mapping(phys_addr, virt_addr);
        mock_pager.allow_get_virtual_address();

        let queue = Queue::<BigSampleItem>::new(mock_pager.get_mock());
        assert_eq!(queue as *const _ as Address, virt_addr.0);

        assert_eq!(queue.head, None);
        assert_eq!(queue.tail, None);

        // the queue reference is dropped; now we can use the pager mutably again for assertions
        mock_pager.checkpoint();
    }

    #[test]
    fn test_enqueue_single_page() {
        // TODO: create a TestQueue which simplifies the following?
        // Allow for spanning multiple pages
        let mut mock_pager = TestPager::new();

        let phys_addr = PhysicalAddress(0x1000);
        mock_pager.allow_allocate_physical(phys_addr);

        let backing_store = Box::new([0u8; 4096]);
        let base_addr = backing_store.as_ptr() as usize as crate::Address;
        let virt_addr = VirtualAddress(base_addr);

        mock_pager.add_mapping(phys_addr, virt_addr);
        mock_pager.allow_get_virtual_address();

        let queue = Queue::<u32>::new(mock_pager.get_mock());
        
        for value in 0..10 {
            queue.enqueue(value);
        }

        for value in 0..10 {
            let result = queue.dequeue();
            assert_eq!(result, Ok(value));
        }
    }
    
    #[test]
    fn test_enqueue_multi_page() {
        assert_eq!(std::mem::size_of::<LinkNode<BigSampleItem>>(), 1024);
        // TODO: create a TestQueue which simplifies the following?
        // Allow for spanning multiple pages
        let mut mock_pager = TestPager::new();

        let phys_addr1 = PhysicalAddress(0x1000);
        let phys_addr2 = PhysicalAddress(0x5000);
        mock_pager.allow_allocate_physical(phys_addr1);
        mock_pager.allow_allocate_physical(phys_addr2);

        let backing_store = Box::new([0u8; 8196]); // 2 pages
        let base_addr1 = backing_store.as_ptr() as usize as crate::Address;
        let base_addr2 = base_addr1 + 4096;
        let virt_addr1 = VirtualAddress(base_addr1);
        let virt_addr2 = VirtualAddress(base_addr2);

        mock_pager.add_mapping(phys_addr1, virt_addr1);
        mock_pager.add_mapping(phys_addr2, virt_addr2);
        mock_pager.allow_get_virtual_address();

        // a single page can only hold 4 BigSampleItem's... and the first page, 
        // because it also contians a header, can only contain 3 BigSampleItems
        let queue = Queue::<BigSampleItem>::new(mock_pager.get_mock());
        
        // TODO: determine why the 7th node allocates another page... it shouldn't
        // first page ___        _____ Second Page
        //               \      /
        //             vvvvv vvvvvvv
        for value in [ 1,2,3,4,5,6,7 ] {
            queue.enqueue(BigSampleItem::new(value));
        }

        for value in 1..=7 {
            let result = queue.dequeue();
            assert_eq!(result, Ok(BigSampleItem::new(value)));
        }
    }
}
