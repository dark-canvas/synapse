use core::sync::atomic::{AtomicU32, AtomicBool, Ordering};
use core::cell::UnsafeCell;
use core::ops::{Drop, Deref, DerefMut};
use crate::errors::ErrCode;

struct LockValue(u32);

const UNLOCKED : LockValue = LockValue(0);

#[allow(dead_code)]
pub struct CpuMutex<T, const RECURSIVE: bool = false> {
    // TODO: need to be able to provide the mutex to multiple code locations (i.e., multiple owners)
    // Which means `owner` needs to be a pointer to an AtomicU32 (and poisoned and data)
    // Or just pass a reference to the CpuMutex to the owners
    owner: AtomicU32, 
    poisoned: AtomicBool,
    data: UnsafeCell<T>,
}

#[allow(dead_code)]
pub struct CpuMutexGuard<'a, T, const RECURSIVE: bool = false> {
    mutex: &'a CpuMutex<T, RECURSIVE>,
}

unsafe impl<'a, T, const RECURSIVE: bool> Sync for CpuMutexGuard<'a, T, RECURSIVE> {}
unsafe impl<'a, T, const RECURSIVE: bool> Send for CpuMutexGuard<'a, T, RECURSIVE> {}

impl<'a, T, const RECURSIVE: bool> Drop for CpuMutexGuard<'a, T, RECURSIVE> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}

impl<'a, T, const RECURSIVE: bool> Deref for CpuMutexGuard<'a, T, RECURSIVE> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<'a, T, const RECURSIVE: bool> DerefMut for CpuMutexGuard<'a, T, RECURSIVE> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

#[allow(dead_code)]
impl<'a, T, const RECURSIVE: bool> CpuMutexGuard<'a, T, RECURSIVE> {
    fn new(mutex: &'a CpuMutex<T, RECURSIVE>) -> CpuMutexGuard<'a, T, RECURSIVE> {
        CpuMutexGuard{
            mutex: mutex
        }
    }
}

unsafe impl<T, const RECURSIVE: bool> Sync for CpuMutex<T, RECURSIVE> {}
unsafe impl<T, const RECURSIVE: bool> Send for CpuMutex<T, RECURSIVE> {}

#[cfg(not(test))]
fn get_cpu_id() -> u32 {
    // if SMP isn't initialized (returns an error) assume we're on the BSP
    crate::arch::x86_64::smp::get_cpu_id().unwrap_or(0)
}

#[cfg(test)]
fn get_cpu_id() -> u32 {
    tests::get_cpu_id()
}

impl LockValue {

    fn new(cpu_id: u32, lock_count: u32) -> Self {
        LockValue((cpu_id + 1) << 16 | lock_count)
    }

    fn get(&self) -> u32 {
        self.0
    }

    fn is_locked(&self) -> bool {
        self.0 != 0
    }

    fn get_cpu_id(&self) -> u32 {
        (self.0 >> 16) - 1
    }

    fn get_lock_count(&self) -> u32 {
        self.0 & 0xFFFF
    }

    fn decrement_lock_count(&self) -> LockValue {
        let cpu_id = self.get_cpu_id();
        let lock_count = self.get_lock_count();
        if lock_count == 0 {
            return UNLOCKED;
        }
        LockValue::new(cpu_id, lock_count - 1)
    }

    fn increment_lock_count(&self) -> LockValue {
        let cpu_id = self.get_cpu_id();
        let lock_count = self.get_lock_count();
        assert_ne!(lock_count, 0xFFFF, "CpuMutex lock count overflow");
        LockValue::new(cpu_id, lock_count + 1)
    }

}

#[allow(dead_code)]
impl<T, const RECURSIVE: bool> CpuMutex<T, RECURSIVE> {
    pub fn new(data: T) -> Self {
        Self {
            owner: AtomicU32::new(UNLOCKED.0),
            data: UnsafeCell::new(data),
            poisoned: AtomicBool::new(false),
        }
    }

    fn get_current_value(&self) -> LockValue {
        LockValue(self.owner.load(Ordering::Relaxed))
    }

    fn get_lock_value(&self) -> (LockValue, LockValue) {
        let cpu_id = get_cpu_id();

        if RECURSIVE {
            // intentionally operate on a copy to keep everything here consistent.
            // current_value could change via any other thread, but that's okay, and 
            // will be handled by the cmpxchange in the lock() method.
            let current_value = self.get_current_value();
            if current_value.is_locked() &&
                cpu_id == Self::get_cpu(&current_value) {
                // TODO: assert that the lock count isn't maxed out (i.e., 0xFFFF) and panic if it is
                // increment the recursive count
                return (current_value.increment_lock_count(), current_value);
            }
        }

        return (LockValue::new(cpu_id, 0), UNLOCKED);
    }

    fn get_cpu(lock_value: &LockValue) -> u32 {
        return lock_value.get_cpu_id();
    }

    // TODO: add other locking methods:
    //   try_lock() - exit immediately if it couldn't lock
    //   try_lock_for(duration) - if locked keep trying up to duration

    pub fn lock(&self) -> Result<CpuMutexGuard<'_, T, RECURSIVE>, ErrCode> {
        let (mut new_lock_value, mut current_lock_value) = self.get_lock_value();
        loop {
            match self.owner.compare_exchange(
                current_lock_value.0,
                new_lock_value.0,
                Ordering::Acquire,
                Ordering::Relaxed) {
            
                Ok(_) => return Ok( CpuMutexGuard::new(self) ),
                Err(_lock_value) => {
                    if RECURSIVE {
                        (new_lock_value, current_lock_value) = self.get_lock_value();
                    }
                    core::hint::spin_loop();
                    continue;
                }
            }
        }
    }

    fn unlock(&self) {
        let current_lock_value = self.get_current_value();
        let cpu_id = get_cpu_id();
        assert_eq!(Self::get_cpu(&current_lock_value), cpu_id, 
            "CpuMutex unlocked by a different CPU than the one that locked it");

        let new_lock_value = current_lock_value.decrement_lock_count();
        match self.owner.compare_exchange(
            current_lock_value.0,
            new_lock_value.0,
            Ordering::Acquire,
            Ordering::Relaxed) {

            Ok(_) => return,
            Err(_current_lock_value) => self.poisoned.store(true, Ordering::Relaxed), // lock is poisoned!!!

        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CpuMutex, UNLOCKED};
    use core::sync::atomic::Ordering;
    use std::sync::Mutex;

    static CPU_ID: Mutex<u32> = Mutex::new(0);

    pub fn get_cpu_id() -> u32 {
        *CPU_ID.lock().unwrap()
    }


    #[test]
    fn guard_provides_access_and_unlocks_on_drop() {
        let mutex: CpuMutex<i32> = CpuMutex::new(1);

        {
            let mut guard = mutex.lock().unwrap();
            assert_eq!(*guard, 1);
            *guard = 2;
            assert_ne!(mutex.owner.load(Ordering::Relaxed), UNLOCKED.0);
        }

        assert_eq!(mutex.owner.load(Ordering::Relaxed), UNLOCKED.0);
        assert_eq!(*mutex.lock().unwrap(), 2);
    }

    #[test]
    fn recursive_locking_works() {
        let mutex: CpuMutex<i32, true> = CpuMutex::new(1);

        {
            let mut guard1 = mutex.lock().unwrap();
            assert_eq!(*guard1, 1);
            {
                let mut guard2 = mutex.lock().unwrap();
                assert_eq!(*guard2, 1);
                *guard2 = 2;
                assert_ne!(mutex.owner.load(Ordering::Relaxed), UNLOCKED.0);
            }
            assert_ne!(mutex.owner.load(Ordering::Relaxed), UNLOCKED.0);
        }
        assert_eq!(mutex.owner.load(Ordering::Relaxed), UNLOCKED.0);
        assert_eq!(*mutex.lock().unwrap(), 2);
    }
}