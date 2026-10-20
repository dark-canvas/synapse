use super::TaskHandle;

pub trait Task {
    type TaskHandle : TaskHandle;

    fn get_handle(&self) -> Self::TaskHandle;
}