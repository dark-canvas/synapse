mod smp;
pub mod acpi_handler;
pub mod trampoline;
pub mod relocation;
pub mod cpu_state;
pub mod cpu_stack;
pub mod per_cpu_data;

pub use self::smp::init;
pub use self::smp::SMP_INITIALIZED;