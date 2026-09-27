use crate::errors::ErrCode;

use super::Task;

pub trait Scheduler {
    type Task: Task;
    fn add_task(&mut self, task: Self::Task) -> Result<(), ErrCode>;

    // fn get_next() -> &Task
    // fn get_current() -> &Task;
    // fn switch_to_task(task: &Task);
}
