mod smp;
mod acpi_handler;
mod trampoline;
mod relocation;
mod cpu_state;
mod cpu_stack;
pub(crate) mod per_cpu_data;

pub use smp::init;
pub use smp::SMP_INITIALIZED;
pub use cpu_state::CpuState;
pub use cpu_state::State;
pub use cpu_state::get_cpu_id;
pub use cpu_state::get_current_task_id;
pub use cpu_stack::CpuStack;