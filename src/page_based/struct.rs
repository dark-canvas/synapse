pub struct PageBasedStruct<T> {
    header: T,
}

impl<T> PageBasedStruct<T> {
    pub fn new(header: T) {

        // TOOD: use public API
        let page_address = X86_PAGER.get().unwrap().allocate_4kb_page().unwrap();

        let 

        PageBasedStruct {
            header: T
        }
    }
}

// type PageBasedList = PageBased<(), T> ??
// type Mutex = PageBased<MutexHeader, TaskID>

// PageBasedList = PageBasedStruct<ListHeader> ?
// Semaphore = PageBasedList<SemaphoreHeader, TaskID>
// Mutex = PageBasedList<MutexHeader, TaskID>

// Also need some structure to keep a cache of free'd pages?
// Can this be shared between multiple paged-based primitives