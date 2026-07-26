// TODO: need to have a common Task structure which the arch-specific stuff can extend?

pub trait Scheduler {
    type Task;

    // consumes Task
    fn add_task(&mut self, task: Task) -> Result<(), ErrCode>;

    // fn get_next() -> &Task
    // fn get_current() -> &Task;
    // fn switch_to_task(task: &Task);
}

struct NullScheduler {}

impl Scheduler for NullScheduler {
    fn add_task(&mut self, task: Task) -> Result<(), ErrCode> { Err(ErrCode::Unimplemented) }
}
