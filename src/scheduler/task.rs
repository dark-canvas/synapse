pub type TaskId = u64;

pub trait Task {
    fn get_id(&self) -> TaskId;
}