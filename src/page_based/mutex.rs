use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};

// Things that need solutions:
// - Single atomic for status + count of waiting tasks, or separate
// - Atomic way to add waiting task
//   - locked list, or somehow use cmp-exchange
// - how to handle queue of waiting tasks (page-based-queue)
//   - circular buffer (maximum number of waiting tasks)

// REVISIT: This mutex doesn't wrap any data, like the stock Rust one does.
// It we can create a pattern which allows for these page-paged primitives to stack 
// on each other, then a typical wrapping mutex could just be something like:
// type WrappingMutex<T> = Stacked<T, Mutex> (or something like that)
// I guess I'd need some generic to aide in page-based memory layouts

// would actualy be a queue... FIFO
type Mutex = super::CustomList<MutexHeader>;

pub struct MutexHeader {
    // TODO: encode lock state and number of waiters into a single value?
    // Or just lock state (enum -> unlocked, locked, locked_with_wait_list)
    //   wait list would then require a separate locked list
    //   can there somehow be atomic/thread-safe versions of the page-based list, queue, stack without using typical mutexes?
    state: AtomicUsize;
}

const UNLOCKED: usize = 0;
const LOCKED: usize = 1;

impl Mutex {
    pub fn lock() -> MutexGuard {
        //let mut current = self.state.load(Ordering::Relaxed);
    
        loop {
            match atomic_val.compare_exchange(
                UNLOCKED, 
                LOCKED, 
                Ordering::AcqRel, 
                Ordering::Acquire
            ) {
                Ok(_) => return MutexGuard(&Self);
                Err(actual) => {
                    // unable to lock; presumably the current state has threads waiting on the 
                    // lock, so add us to the queue of waiting thrads (keep in mind it may actually 
                    // become free *AS* we do this)
                    if self.add_to_wait_list(actual) == UNLOCKED {
                        continue;
                    } else {
                        break;
                    }
                }
            }
        }
    }

    fn add_to_wait_list(current_state: usize) -> usize {
        let mut current_state = current_state;
        let mut next_state = current_state + 2;

        // lower bit is locked/unlocked, so to add a waiter, add 2
        loop {
            match atomic_val.compare_exchange(
                current_state, 
                next_state, 
                Ordering::AcqRel, 
                Ordering::Acquire
            ) {
                Ok(_) => {
                    // TODO: need to actually add self to the list of waiting threads, but in an atomic sort of way
                    return next_state;
                }
                Err(actual) => {
                    if actual == UNLOCKED {
                        return UNLOCKED;
                    } else {
                        current_state = actual;
                        next_state = current_state += 2;
                    }
                }
            }
       }
    }
}