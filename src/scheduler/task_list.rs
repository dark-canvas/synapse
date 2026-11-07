/// A generic task list interface
/// 
/// While the interface is called a "list," it certainly doesn't need to be implemented as a list, it 
/// simply needs to be able to provide the next available task to run (if there is one) and allow for 
/// new tasks to be added to the list.
///
/// Should this use &Task or TaskHandle?  In general, passing TaskHandle is probably safer, but incurs 
/// an extra lookup.

use crate::scheduler::TaskHandle;

pub trait TaskList {
    type TaskHandle: TaskHandle;
    
    fn dequeue(&mut self) -> Option<Self::TaskHandle>;
    fn enqueue(&mut self, handle: Self::TaskHandle);
}