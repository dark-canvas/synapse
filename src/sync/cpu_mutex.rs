use core::sync::atomic::{AtomicU32, Ordering};
use core::ops::Drop;
use crate::errors::ErrCode;
use crate::arch::x86_64::smp::cpu_state; // need a better way to do this across arch!  Think of a design!

const UNLOCKED : u32 = 0;

struct CpuMutex {
    owner: AtomicU32,
    poisoned: bool,
}

struct CpuMutexGuard<'a> {
    mutex: &'a mut CpuMutex,
}

impl<'a> Drop for CpuMutexGuard<'a> {
    fn drop(&mut self) {
        self.mutex.Unlock();
    }
}

impl<'a> CpuMutexGuard<'a> {
    fn new(mutex: &'a mut CpuMutex) -> CpuMutexGuard<'a> {
        CpuMutexGuard{
            mutex: mutex
        }
    }
}

impl CpuMutex {
    pub fn new() -> Self {
        Self {
            owner: AtomicU32::new(UNLOCKED),
            poisoned: false,
        }
    }

    fn get_lock_value() -> u32 {
        cpu_state::get_cpu_id().unwrap() as u32 + 1 // CpuId == 0 is valid, but == UNLOCKED, so we can't use it
    }

    pub fn Lock(&mut self) -> Result< CpuMutexGuard, ErrCode > {
        let lock_value = Self::get_lock_value();
        loop {
            match self.owner.compare_exchange(
                UNLOCKED,
                lock_value,
                Ordering::Acquire,
                Ordering::Relaxed) {
            
                Ok(_) => return Ok( CpuMutexGuard::new(self) ),
                Err(_cpu) => continue, // lock is owned by `cpu` now
            
            }
        }
    }

    pub fn Unlock(&mut self) {
        match self.owner.compare_exchange(
            Self::get_lock_value(),
            UNLOCKED,
            Ordering::Acquire,
            Ordering::Relaxed) {
        
            Ok(_) => return,
            Err(_cpu) => self.poisoned = true, // lock is poisoned!!!
        
        }
    }
}