use core::sync::atomic::{AtomicU32, Ordering};
use crate::types::CpuId;

const UNLOCKED : u32 = 0;

struct CpuMutex {
    owner: AtomicU32,
}

impl CpuMutex {
    pub fn new() -> Self {
        Self {
            owner: AtomicU32::new(UNLOCKED)
        }
    }

    pub fn Lock(&mut self, cpu: CpuId) {
        let lock_value = cpu as u32 + 1; // CpuId == 0 is valid, but == UNLOCKED, so we can't use it
        loop {
            match self.owner.compare_exchange(
                UNLOCKED,
                lock_value,
                Ordering::Acquire,
                Ordering::Relaxed) {
            
                Ok(_) => break,
                Err(cpu) => continue, // lock is owned by `cpu` now
            
            }
            core::hint::spin_loop();
        }
    }
}