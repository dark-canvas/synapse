use core::sync::atomic::{AtomicU32, AtomicBool, Ordering};
use core::cell::UnsafeCell;
use core::ops::{Drop, Deref, DerefMut};
use crate::errors::ErrCode;
// TODO: this non-arch code should not be importing arch-specific code!!!
use crate::arch::x86_64::smp::cpu_state; // need a better way to do this across arch!  Think of a design!
use crate::arch::x86_64::smp::SMP_INITIALIZED;
#[allow(dead_code)]
const UNLOCKED : u32 = 0;

#[allow(dead_code)]
pub struct CpuMutex<T> {
    // TODO: need to be able to provide the mutex to multiple code locations (i.e., multiple owners)
    // Which means `owner` needs to be a pointer to an AtomicU32 (and poisoned and data)
    // Or just pass a reference to the CpuMutex to the owners
    owner: AtomicU32, 
    poisoned: AtomicBool,
    data: UnsafeCell<T>,
}

#[allow(dead_code)]
pub struct CpuMutexGuard<'a, T> {
    mutex: &'a CpuMutex<T>,
}

unsafe impl<'a, T> Sync for CpuMutexGuard<'a, T> {}
unsafe impl<'a, T> Send for CpuMutexGuard<'a, T> {}

impl<'a, T> Drop for CpuMutexGuard<'a, T> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}

impl<'a, T> Deref for CpuMutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<'a, T> DerefMut for CpuMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

#[allow(dead_code)]
impl<'a, T> CpuMutexGuard<'a, T> {
    fn new(mutex: &'a CpuMutex<T>) -> CpuMutexGuard<'a, T> {
        CpuMutexGuard{
            mutex: mutex
        }
    }
}

unsafe impl<T> Sync for CpuMutex<T> {}
unsafe impl<T> Send for CpuMutex<T> {}


#[allow(dead_code)]
impl<T> CpuMutex<T> {
    pub fn new(data: T) -> Self {
        Self {
            owner: AtomicU32::new(UNLOCKED),
            data: UnsafeCell::new(data),
            poisoned: AtomicBool::new(false),
        }
    }

    fn get_lock_value() -> u32 {
        // TODO: what if SMP is not initialized when it's locked, but *IS* when unlocked...
        // possibly need to save the cpu_num into the MutexLockGuard
        if !SMP_INITIALIZED.load(Ordering::Relaxed) {
            return 1;
        }

        cpu_state::get_cpu_id().unwrap() as u32 + 1 // CpuId == 0 is valid, but == UNLOCKED, so we can't use it
    }

    // TODO: add other locking methods:
    //   try_lock() - exit immediately if it couldn't lock
    //   try_lock_for(duration) - if locked keep tryig up to duration

    pub fn lock(&self) -> Result<CpuMutexGuard<'_, T>, ErrCode> {
        let lock_value = Self::get_lock_value();
        loop {
            match self.owner.compare_exchange(
                UNLOCKED,
                lock_value,
                Ordering::Acquire,
                Ordering::Relaxed) {
            
                Ok(_) => return Ok( CpuMutexGuard::new(self) ),
                Err(_cpu) => { // lock is owned by `cpu` now
                    core::hint::spin_loop();
                    continue;
                }
            }
        }
    }

    fn unlock(&self) {
        match self.owner.compare_exchange(
            Self::get_lock_value(),
            UNLOCKED,
            Ordering::Acquire,
            Ordering::Relaxed) {

            Ok(_) => return,
            Err(_cpu) => self.poisoned.store(true, Ordering::Relaxed), // lock is poisoned!!!

        }
    }
}