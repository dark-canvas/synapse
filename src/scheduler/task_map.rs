use crate::errors::ErrCode;

use super::Task;

pub trait TaskMap {
    type Task: Task;
    type TaskHandle;

    fn allocate(&mut self) -> Result<&'static mut Self::Task, ErrCode>;
    fn free(&mut self, task: &Self::Task) -> Result<(), ErrCode>;
    fn get(&self, handle: Self::TaskHandle) -> Result<&'static Self::Task, ErrCode>;
    fn get_mut(&self, handle: Self::TaskHandle) -> Result<&'static mut Self::Task, ErrCode>;
}