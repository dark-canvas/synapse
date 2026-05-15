
pub mod pager;
pub mod idt;
pub mod gdt;
pub mod pit;
pub mod x2apic;

use satus_struct::config::Config;
use self::pager::Pager;
use x86_64::instructions::interrupts;

pub fn init(config: &Config) {
    println!("Initializing x86_64 architecture-specific components...");

    gdt::init();
    idt::init_idt();
    x2apic::init();
    pit::init();

    interrupts::enable();

    println!("Creating pager...");
    let pager = Pager::new(&config);
    
    pager::run_time_tests(&pager);
}