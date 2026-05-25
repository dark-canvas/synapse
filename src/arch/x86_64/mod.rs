
pub mod pager;
pub mod idt;
pub mod gdt;
pub mod pit;
pub mod x2apic;

use crate::pager::PAGER;

use satus_struct::config::Config;
use self::pager::Pager;
use x86_64::instructions::interrupts;
use spin::Once;

static X86_PAGER: Once<Pager> = Once::new();

pub fn init(config: &Config) {
    println!("Initializing x86_64 architecture-specific components...");

    gdt::init();
    idt::init_idt();
    x2apic::init();
    pit::init();

    interrupts::enable();

    println!("Creating pager...");
    X86_PAGER.call_once(|| { Pager::new(&config) });
    *PAGER.borrow_mut() = X86_PAGER.get().unwrap();
    pager::run_time_tests(X86_PAGER.get().unwrap());
}