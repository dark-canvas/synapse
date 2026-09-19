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

const TASK_MAP_MAX_TASKS: usize = 1_048_576; // 1M for now; can likely support more

const TASK_MAP_ARRAY_SIZE: usize = TASK_MAP_MAX_TASKS * core::mem::size_of::<Task>();
const TASK_MAP_FREE_STACK_SIZE: usize = TASK_MAP_MAX_TASKS * core::mem::size_of::<TaskHandle>();

pub type TaskHandle = u32;

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
    assert!(TASK_MAP_ARRAY_SIZE + TASK_MAP_FREE_STACK_SIZE <= TASK_MAP_SIZE, "Task map is too small!");
    assert!(TASK_MAP_MAX_TASKS <= TaskHandle::MAX as usize, "Task map larger than TaskHandle can support!");
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
            return self.construct(self.tasks.get_mut(handle as usize)?);
        } else if let Ok(task) = self.tasks.get_mut(self.next_handle as usize) {
            self.next_handle += 1;
            return self.construct(task);
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

    fn construct(&self, task: &'static mut Task) -> Result<&'static Task, ErrCode> {
        // Initialize the stack (TODO: make this runtime configurable)
        for byte in task.kernel_stack.iter_mut() {
            *byte = 0xa5;
        }
        // Initialize the IO bitmap (by defualt tasks have no IO permissions, so we set all bits to 0)
        task.io_bitmap.fill(0x0);
        // TODO: create a separate address space for this task
        Ok(task)
    }
}

