#[cfg(target_arch = "x86_64")]
pub mod x86_64; // TODO: remove the pub (more virtual and physical address types up to the portable layer)

use satus_struct::config::Config;

pub fn init(config: &Config) {
    println!("Initializing architecture-specific components...");

    #[cfg(target_arch = "x86_64")]
    x86_64::init(config);
}