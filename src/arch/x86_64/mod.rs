// TODO: this should all be private...
mod gdt;
mod global_data;
mod idt;
pub mod pager;
mod pit;
mod scheduler;
pub mod smp;
mod task_map;
#[macro_use]
mod util;
mod x2apic;

use crate::arch::x86_64::scheduler::yield_task;
use crate::pager::PAGER;

use satus_struct::config::Config;
use self::pager::Pager;
use self::scheduler::Scheduler;
use self::smp::cpu_state::{CpuState, State};
use x86_64::instructions::interrupts;
use spin::Once;
use crate::types::CpuId;
use crate::arch::x86_64::task_map::TaskMap;
use global_data::GlobalData;
use crate::sync::cpu_mutex::CpuMutex;

static X86_PAGER: Once<Pager> = Once::new();

// TODO: find a way to share this with the APs... create the CpuState for the BSP here 
// and then call a init_core() method which passes in the CpuState.
// BSP-only functions (eg. smp::init()) can be called only if CpuId == 0.
pub fn init(config: &Config) {
    println!("Initializing x86_64 architecture-specific components...");

    init_core(0, config);
}

pub fn init_core(cpu_id: CpuId, config: &Config) {

    //let config = Config::from_page(state.config);
    let is_bsp = cpu_id == 0;

    gdt::init(cpu_id);
    idt::init_idt();
    x2apic::init();
    pit::init();

    if is_bsp {
        println!("Creating pager...");
        X86_PAGER.call_once(|| { Pager::new(&config) });
        *PAGER.borrow_mut() = X86_PAGER.get().unwrap();
        
        println!("Creating global data");
        GlobalData::init();
        GlobalData::get().task_map = CpuMutex::new( TaskMap::new(*PAGER.borrow_mut()).unwrap() );
    }
 
    // TODO: don't do this yet, as the timer interrupt will modify the contents of the 
    // CURRENT_TASK glboal as well, which will mess up our yield_task() testing
    interrupts::enable();
    //interrupts::disable();

    if is_bsp {
        // the per-cpu-state is created here (for all CPUs), it needs to have a unique scheduler
        // SMP requires pager to be initialized first
        // SMP also requires the timer interrupt setup (for delays)
        smp::init(&config); // TODO: pass in scheduler and taskmap? (need to set in per-state data)
    }

    let cpu_state = CpuState::get_local_cpu_state();

    // must be initialized after smp::init() as it uses cpu_mutex (to access the task map)
    cpu_state.scheduler = Scheduler::new(/*&GlobalData::get().task_map*/);
    // todo: scheduler::init() instead

    // start a new task for each of the tests?
    // allocate a couple pages for the stack
    // create a task struction
    // add it to the scheduler

    if is_bsp {
        // as a test, create another task and yield to it, then yield back to the kernel task
        cpu_state.scheduler.new_task(|| 
            {
                loop {
                    println!("Hello from the test task!");
                    yield_task();
                }
            }).unwrap();
        pager::run_time_tests(X86_PAGER.get().unwrap());
    }
}
