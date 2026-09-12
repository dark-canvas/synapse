use crate::page_based::node_allocator::NodeAllocator;
use crate::errors::ErrCode;

pub struct Queue<T: Copy> {
    head: Option<*mut LinkNode<T>>,
    tail: Option<*mut LinkNode<T>>,
    allocator: NodeAllocator< LinkNode<T> >,
}

struct LinkNode<T: Copy> {
    item: T,
    next: Option<*mut LinkNode<T>>,
}

impl<T: Copy> Queue<T> {
    pub fn new() -> Self {
        Self {
            head: None,
            tail: None,
            allocator: NodeAllocator::new(),
        }
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
    use crate::pager::MOCK_PAGER_INSTANCE;

    #[test]
    fn test_create_stack() {
        // create expectations using MOCK_PAGER_INSTANCE
        // test get_pager() is designed to return it
    }
}
