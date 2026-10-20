use crate::errors::ErrCode;
use crate::sync::cpu_mutex::CpuMutex;

use super::Task;
use super::TaskHandle;
use super::TaskMap;

pub trait Scheduler {
    type Task: Task;
    type TaskHandle: TaskHandle;
    type TaskMap: TaskMap;

    // TODO: need to figure this out... should TaskMap be a portable trait

    fn add_task(&mut self, task: &Self::Task) -> Result<(), ErrCode>;

    fn get_task_map(&self) -> &CpuMutex< Self::TaskMap >;

    // fn get_next() -> &Task
    // fn get_current() -> &Task;
    // fn switch_to_task(task: &Task);

    fn context_switch(&mut self) {
        let task_map = self.get_task_map();

        
    }
}
