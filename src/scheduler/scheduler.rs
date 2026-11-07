/// The scheduler is the manager of all the tasks (also called processes, threads) on the system.
/// It maanages both running (of which there is only ever one per CPU) and waiting tasks.  Waiting 
/// tasks can be in a "ready to run" state, or a blocked state (waiting on a resource to become 
/// available).
/// 
/// A separate Scheduler instance will run on each available CPU, which will allow for parallel 
/// execution of code.  Each scheduler will also regularly be asked (typically via a separate timer) 
/// for decisions on how long the current task should run, and which task should run next, which allows 
/// for pre-emptive multitasking (effectively simulating parallel execution within a single CPU)
/// 
/// A task structure contains all the data necessary to run a piece of code on the target platform.
/// The task will differ per platform and could contain stacks or other large pieces of data, and so 
/// the TaskHandle exists as a short-form method of referencing the task, and is provided to the 
/// various scheduling algorithms in order to prevent requiring Task pointers to be passed around 
/// (potentially through different memory address spaces).
/// 
/// The TaskMap exists to convert a TaskHandle into a concrete Task structure.  The TaskMap is global 
/// to all CPUs (all Tasks in the system will exist in a single TaskMap) and therefore access to it must 
/// be synchronized 
/// 
/// In order to support latency requirements (such as from drivers for hardware with strict timing 
/// requiremens) Task execution will be prioritiized.  Each task is set to a specific priority, and 
/// the scheduler consults the various priorities when making it's decision on which Task to run next, 
/// and for how long.  The scheduling algorithm itself is defined within the trait and inherited by 
/// each implementation, ensuring that the scheduler on each platform behaves similarly.  The per-platform 
/// scheduler only needs to be concerned with the per-platform logistics of starting and stopping a 
/// task on the current CPU.
use crate::errors::ErrCode;
use crate::sync::cpu_mutex::CpuMutex;

use super::Task;
use super::TaskHandle;
use super::TaskMap;
use super::TaskList;
// REVISIT: does this interface need to know about the TaskMap at all?
pub trait Scheduler {
    type Task: Task;
    type TaskHandle: TaskHandle;
    type TaskMap: TaskMap;
    type TaskList: TaskList<TaskHandle = Self::TaskHandle>;

    fn add_task(&mut self, task: &Self::Task) -> Result<(), ErrCode>;

    // REVISIT: do we really want to expose this?  Do we want to impose that it must be wrapped in a 
    // CpuMutex?
    fn get_task_map(&self) -> &CpuMutex< Self::TaskMap >;

    fn get_active_tasks(&mut self) -> &mut Self::TaskList;

    fn switch_to_task(&mut self, task: Self::TaskHandle);

    fn context_switch(&mut self) {
        let active_tasks = self.get_active_tasks();
        if let Some(task_handle) = active_tasks.dequeue() {
            self.switch_to_task(task_handle);
        }
    }
}
