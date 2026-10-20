// TODO: this should all be private...
mod gdt;
mod global_data;
mod idt;
pub mod pager;
mod pit;
mod scheduler;
pub mod smp;
#[macro_use]
mod util;
mod x2apic;

use crate::arch::x86_64::scheduler::yield_task;
use crate::arch::x86_64::x2apic::timer::timer_init;
use crate::pager::PAGER;

use satus_struct::config::Config;
use self::pager::Pager;
use self::scheduler::Scheduler;
use self::smp::{CpuState, State};
use x86_64::instructions::interrupts;
use spin::Once;
use crate::types::CpuId;
use self::scheduler::TaskMap;
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

    gdt::init(cpu_id); // setups up the TSS, so needs to be initialized before scheduler and interrupts
    idt::init_idt();   // sets up all the execption handlers, so should be done early
    //pit::init();       // current is a dependency for smp::init, for delays -- need to replace it

    // pager needs to be one of the first things created as nearly anything which requires any 
    // of the following will require it:
    // - virtual <-> physical address translation
    // - memory allocations (only per-page is supported in the kernel)
    if is_bsp {
        println!("Creating pager...");
        X86_PAGER.call_once(|| { Pager::new(&config) });
        *PAGER.borrow_mut() = X86_PAGER.get().unwrap();
        
        // TODO: rename to GlobalState to align with CpuState?
        println!("Creating global data");
        GlobalData::init();
        GlobalData::get().task_map = CpuMutex::new( TaskMap::new(*PAGER.borrow_mut()).unwrap() );
    }
 
    // TODO: don't do this yet, as the timer interrupt will modify the contents of the 
    // CURRENT_TASK glboal as well, which will mess up our yield_task() testing
    interrupts::enable();

    if is_bsp {
        // the per-cpu-state is created here (for all CPUs), it needs to have a unique scheduler
        // SMP requires pager to be initialized first
        // SMP also requires the timer interrupt setup (for delays) -- TODO: switch to someting else for delays!!!
        // the x2apic needs cpu-state created first in order to save off the lapic struct
        smp::init(&config); // TODO: pass in scheduler and taskmap? (need to set in per-state data)
    }

    x2apic::init();


    let cpu_state = CpuState::get_local_cpu_state();

    // must be initialized after smp::init() as it uses cpu_mutex (to access the task map)
    cpu_state.scheduler = Scheduler::new(/*&GlobalData::get().task_map*/);
    // todo: scheduler::init() instead

    timer_init();

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
                    //yield_task();
                }
            }).unwrap();
        pager::run_time_tests(X86_PAGER.get().unwrap());
    } else {
        loop {
            println!("Hello from CPU {}", cpu_id);
            core::hint::spin_loop();
        }
    }
}
