#[cfg(target_arch = "x86_64")]
mod x86_64;

use satus_struct::config::Config;

pub fn init(config: &Config) {
    println!("Initializing architecture-specific components...");

    #[cfg(target_arch = "x86_64")]
    x86_64::init(config);
}