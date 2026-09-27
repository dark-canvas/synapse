use crate::errors::ErrCode;

use super::Task;

pub trait TaskMap {
    type Task: Task;

    fn allocate(&self) -> Result<&'static Self::Task, ErrCode>;
    fn free(&self, task: &Self::Task) -> Result<(), ErrCode>;
}