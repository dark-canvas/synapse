
pub mod pager;
pub mod idt;
pub mod gdt;

use satus_struct::config::Config;
use self::pager::Pager;

pub fn init(config: &Config) {
    println!("Initializing x86_64 architecture-specific components...");

    gdt::init();
    idt::init_idt();

    println!("Creating pager...");
    let pager = Pager::new(&config);
    
    pager::run_time_tests(&pager);
}