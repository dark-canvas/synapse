// REVISIT: does this belong here?  Or should there just be a generic handle?  Or trait?
// Or is TaskId a u64... and the impl splits it into a index/id ?
pub type TaskId = u64;

pub trait Task {
    fn get_id(&self) -> TaskId;
}