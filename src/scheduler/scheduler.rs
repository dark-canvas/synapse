use crate::errors::ErrCode;

use super::Task;
//use super::TaskMap;

pub trait Scheduler {
    type Task: Task;
    //type TaskMap: TaskMap;

    // TODO: need to figure this out... should TaskMap be a portable trait

    fn add_task(&mut self, task: &Self::Task) -> Result<(), ErrCode>;

    // fn get_next() -> &Task
    // fn get_current() -> &Task;
    // fn switch_to_task(task: &Task);
}
