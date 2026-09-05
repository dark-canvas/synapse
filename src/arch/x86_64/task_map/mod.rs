//! TaskMap
//! The task map, while actually a fixed-size array, is a mechanism to map a task id, or handle, to the underlying 
//! task structure.
//! The task structure contains all the data necessary to suspend a task, and then later restore it to a running 
//! state.
//! The task map consists of an array of task structures, and a stack of free task handles.

use crate::arch::x86_64::util::register_snapshot::RegisterSnapshot;
use crate::arch::x86_64::pager::{PhysicalAddress, VirtualAddress};
use crate::errors::ErrCode;
use crate::pager::on_demand_array::OnDemandArray;
use crate::pager::on_demand_stack::OnDemandStack;

const TASK_MAP_BASE_ADDRESS: VirtualAddress = VirtualAddress(0xFFFFFFF010000000);
const TASK_MAP_TOP: VirtualAddress = VirtualAddress(0xFFFFFFFFFFFFFFFF);
const TASK_MAP_SIZE: usize = TASK_MAP_TOP.0 as usize - TASK_MAP_BASE_ADDRESS.0 as usize + 1;

// TODO: allocating more than the struct size is wasteful
//const TASK_MAP_ITEM_SIZE: usize = 32768; // Can support ~2M tasks (although the free stack eats into that)
const TASK_MAP_MAX_TASKS: usize = 1_048_576; // 1M for now; can likely support more

const TASK_MAP_ARRAY_SIZE: usize = TASK_MAP_MAX_TASKS * core::mem::size_of::<Task>();
const TASK_MAP_FREE_STACK_SIZE: usize = TASK_MAP_MAX_TASKS * core::mem::size_of::<TaskHandle>();

pub type TaskHandle = u32;

// TODO: a copy is inefficient with a struct this size... need to figure out a better way to initialize 
// in place; the works for now, to get off the ground, though.
#[derive(Debug, Clone, Copy)]
pub struct Task {
    kernel_stack: [u8; 16*1024],
    io_bitmap: [u8; 8193],
    cr3: PhysicalAddress,
    gs_base: VirtualAddress,
    registers: RegisterSnapshot,
    //fp_registers: FPRegisterSnapshot,
    //avx_snapshot: AVXRegisterSnapshot,
}

const _: () = {
    //assert!(core::mem::size_of::<Task>() < TASK_MAP_ITEM_SIZE, "Task is too large!");
    assert!(TASK_MAP_ARRAY_SIZE + TASK_MAP_FREE_STACK_SIZE <= TASK_MAP_SIZE, "Task map is too small!");
};

// TODO: this is shared between CPUs and so will need a CPU mutex
pub struct TaskMap {
    tasks: OnDemandArray::<Task>,
    free_stack: OnDemandStack::<TaskHandle>,
    next_handle: TaskHandle,
}

impl TaskMap {

    pub fn new(array_base: VirtualAddress, free_stack_base: VirtualAddress, max_handles: usize) -> Result<TaskMap, ErrCode> {
        Ok(TaskMap {
            tasks: OnDemandArray::new(array_base, max_handles),
            free_stack: OnDemandStack::new(free_stack_base, max_handles),
            next_handle: 0,
        })
    }

    pub fn get_task(&self, handle: TaskHandle) -> Result<&'static Task, ErrCode> {
        self.tasks.get(handle as usize)
    }

    pub fn new_task(&mut self) -> Result<&'static Task, ErrCode> {
        if let Ok(handle) = self.free_stack.pop() {
            // Implementation for creating a new task
            return Err(ErrCode::Unimplemented);
        } else if let Ok(handle) = self.tasks.get(self.next_handle as usize) {
            self.next_handle += 1;
            // Implementation for creating a new task
            return Err(ErrCode::Unimplemented);
        } else {
            return Err(ErrCode::OutOfHandles);
        }
    }

    pub fn free_task(&mut self, handle: TaskHandle) -> Result<(), ErrCode> {
        if handle == 0 || handle >= self.next_handle {
            return Err(ErrCode::InvalidHandle);
        }
        if handle == self.next_handle - 1 {
            self.next_handle -= 1;
        } else {
            self.free_stack.push(handle)?;
        }
        Ok(())
    }
}

