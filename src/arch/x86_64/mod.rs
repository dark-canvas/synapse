pub mod gdt;
pub mod idt;
pub mod pager;
pub mod pit;
pub mod scheduler;
pub mod smp;
#[macro_use]
pub mod util;
pub mod x2apic;

use crate::pager::PAGER;

use satus_struct::config::Config;
use self::pager::Pager;
use self::scheduler::Scheduler;
use self::smp::cpu_state::{CpuState, State};
use x86_64::instructions::interrupts;
use spin::Once;
use crate::types::CpuId;

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
    let isBsp = cpu_id == 0;

    // Need to determine how many of these can be (or need to be) initialized per core, 
    // or if it's fine just doing on the BSP
    gdt::init(cpu_id);
    idt::init_idt();
    x2apic::init();
    pit::init();

    if isBsp {
        println!("Creating pager...");
        X86_PAGER.call_once(|| { Pager::new(&config) });
        *PAGER.borrow_mut() = X86_PAGER.get().unwrap();
    }

    Scheduler::new();
    // todo: scheduler::init() instead

    // TODO: don't do this yet, as the timer interrupt will modify the contents of the 
    // CURRENT_TASK glboal as well, which will mess up our yield_task() testing
    interrupts::enable();
    //interrupts::disable();

    if isBsp {
        // SMP requires pager to be initialized first
        // SMP also requires the timer interrupt setup (for delays)
        smp::init(&config);
    }

    // start a new task for each of the tests?
    // allocate a couple pages for the stack
    // create a task struction
    // add it to the scheduler

    if isBsp {
        pager::run_time_tests(X86_PAGER.get().unwrap());
    }
}
