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
use x86_64::instructions::interrupts;
use spin::Once;

static X86_PAGER: Once<Pager> = Once::new();

pub fn init(config: &Config) {
    println!("Initializing x86_64 architecture-specific components...");

    gdt::init();
    idt::init_idt();
    x2apic::init();
    pit::init();
    smp::init(&mut config.get_cpu_config());

    println!("Creating pager...");
    X86_PAGER.call_once(|| { Pager::new(&config) });
    *PAGER.borrow_mut() = X86_PAGER.get().unwrap();

    Scheduler::new();
    // todo: scheduler::init() instead

    // TODO: don't do this yet, as the timer interrupt will modify the contents of the 
    // CURRENT_TASK glboal as well, which will mess up our yield_task() testing
    interrupts::enable();
    //interrupts::disable();

    // start a new task for each of the tests?
    // allocate a couple pages for the stack
    // create a task struction
    // add it to the scheduler

    pager::run_time_tests(X86_PAGER.get().unwrap());
}
