//! TaskMap
//! The task map, while actually a fixed-size array, is a mechanism to map a task id, or handle, to the underlying 
//! task structure.
//! The task structure contains all the data necessary to suspend a task, and then later restore it to a running 
//! state.
//! The task map consists of an array of task structures, and a stack of free task handles.

use crate::errors::ErrCode;
use crate::pager::VirtualAddress;
use crate::pager::Pager;
use crate::pager::OnDemandArray;
use crate::pager::OnDemandStack;
use crate::arch::x86_64::scheduler::{Task, TaskHandle};
use crate::scheduler::Task as TaskInterface;
use crate::scheduler::TaskHandle as _;
use crate::scheduler::TaskMap as TaskMapInterface;

pub const TASK_MAP_BASE_ADDRESS: VirtualAddress = VirtualAddress(0xFFFFFFF010000000);
const TASK_MAP_TOP: VirtualAddress = VirtualAddress(0xFFFFFFFFFFFFFFFF);
const TASK_MAP_SIZE: usize = TASK_MAP_TOP.0 as usize - TASK_MAP_BASE_ADDRESS.0 as usize + 1;

const TASK_MAP_MAX_TASKS: usize = 1_048_576; // 1M for now; can likely support more

const TASK_MAP_ARRAY_SIZE: usize = TASK_MAP_MAX_TASKS * core::mem::size_of::<Task>();
const TASK_MAP_FREE_STACK_SIZE: usize = TASK_MAP_MAX_TASKS * core::mem::size_of::<TaskHandle>();
const TASK_MAP_FREE_STACK_BASE: VirtualAddress = VirtualAddress(TASK_MAP_TOP.0 - TASK_MAP_FREE_STACK_SIZE as u64 + 1);

const _: () = {
    assert!(TASK_MAP_ARRAY_SIZE + TASK_MAP_FREE_STACK_SIZE <= TASK_MAP_SIZE, "Task map is too small!");
    assert!(TASK_MAP_MAX_TASKS <= TaskHandle::MAX, "Task map larger than TaskHandle can support!");
};

pub struct TaskMap<P>
where
    P: Pager + Copy,
{
    tasks: OnDemandArray<P, Task>,
    free_stack: OnDemandStack<P, TaskHandle>,
    next_available_index: usize,
}

impl<P> TaskMap<P>
where
    P: Pager + Copy,
{
    pub fn new(pager: P) -> Result<TaskMap<P>, ErrCode> {
        Self::new_at(pager,
            TASK_MAP_BASE_ADDRESS,
            TASK_MAP_FREE_STACK_BASE,
            TASK_MAP_MAX_TASKS)
    }

    pub fn new_at(pager: P, array_base: VirtualAddress, free_stack_base: VirtualAddress, max_handles: usize) -> Result<TaskMap<P>, ErrCode> {
        Ok(TaskMap {
            tasks: OnDemandArray::new(pager, array_base, max_handles),
            free_stack: OnDemandStack::new(pager, free_stack_base, max_handles),
            next_available_index: 0,
        })
    }

    pub fn get_index(&self, task: &Task) -> Result<usize, ErrCode> {
        let base_address = self.tasks.get_base_address();
        let task_address = task as *const Task as usize;
        let base_address_usize = base_address.0 as usize;
        if task_address < base_address_usize || task_address >= base_address_usize + TASK_MAP_ARRAY_SIZE {
            return Err(ErrCode::InvalidHandle);
        }
        let index = (task_address - base_address_usize) / core::mem::size_of::<Task>();
        Ok(index)
    }

    fn construct(&self, task: &'static mut Task) -> Result<&'static mut Task, ErrCode> {
        // Initialize the stack (TODO: make this runtime configurable)
        for byte in task.kernel_stack.iter_mut() {
            *byte = 0xa5;
        }
        // TODO: provide a means to allocate this, and perform the paging and TSS modifications to 
        // make it work
        task.io_bitmap = None;
        let generation = if task.get_handle().is_null() {
            0
        } else {
            task.get_handle().get_generation() + 1
        };
        task.id = TaskHandle::new(self.get_index(task)?, generation);

        Ok(task)
    }
}

impl<P> TaskMapInterface for TaskMap<P>
where
    P: Pager + Copy,
{
    type Task = Task;
    type TaskHandle = TaskHandle;

    fn allocate(&mut self) -> Result<&'static mut Self::Task, ErrCode> {
        if let Ok(handle) = self.free_stack.pop() {
            // Implementation for creating a new task
            return self.construct(self.tasks.get_mut(handle.get_index())?);
        } else if let Ok(task) = self.tasks.get_mut(self.next_available_index as usize) {
            self.next_available_index += 1;
            return self.construct(task);
        } else {
            return Err(ErrCode::OutOfHandles);
        }
    }
    fn free(&mut self, task: &Self::Task) -> Result<(), ErrCode> {
        let handle = task.get_handle();
        let index = handle.get_index();
        if index >= self.next_available_index {
            return Err(ErrCode::InvalidHandle);
        }
        if index == self.next_available_index - 1 {
            self.next_available_index -= 1;
        } else {
            self.free_stack.push(handle)?;
        }
        Ok(())
    }

    fn get_mut(&self, handle: Self::TaskHandle) -> Result<&'static mut Self::Task, ErrCode> {
        let task = self.tasks.get_mut(handle.get_index())?;
        if task.get_handle() == handle {
            return Ok(task);
        } else {
            return Err(ErrCode::InvalidHandle);
        }
    }

    fn get(&self, handle: Self::TaskHandle) -> Result<&'static Self::Task, ErrCode> {
       match self.get_mut(handle) {
            Ok(r) => Ok(r),
            Err(e) => Err(e),
        }
    }
}
