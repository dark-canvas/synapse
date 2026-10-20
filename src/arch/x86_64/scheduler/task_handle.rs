use core::fmt::{self, Display};
use crate::scheduler::TaskHandle as TaskHandleInterface;

#[derive(Copy, Clone, PartialEq)]
pub struct TaskHandle(u64);

impl TaskHandle {
    pub const MAX: usize = 0xffffffff;

    pub fn new(index: usize, generation: u32) -> Self {
        Self(((generation as u64) << 32) | (index as u64) + 1)
    }

    // 0 is not a valid index
    pub fn get_index(&self) -> usize {
        self.0 as usize & Self::MAX - 1
    }

    pub fn get_generation(&self) -> u32 {
        (self.0 >> 32) as u32
    }
}

impl TaskHandleInterface for TaskHandle {
    // index field of zero isn't valid (an index of 0 is stored as a 1)
    fn is_null(&self) -> bool {
        self.0 == 0
    }

    fn set_null(&mut self) {
        self.0 = 0;
    }
}

impl Display for TaskHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}|{}]", self.get_generation(), self.get_index())
    }
}