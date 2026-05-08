
pub mod pager;
pub mod idt;

use satus_struct::config::Config;
use self::pager::Pager;

pub fn init(config: &Config) {
    println!("Initializing x86_64 architecture-specific components...");

    println!("Creating pager...");
    let pager = Pager::new(&config);

    idt::init_idt();
    
    pager::run_time_tests(&pager);
}