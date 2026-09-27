pub type TaskId = u32;

pub trait Task {
    fn get_id(&self) -> TaskId;
}