/// The page_based module contains primitive structures and collections that rely only on a page-based 
/// memory allocator.  These structures are designed to be used in the kernel where a byte-granularity 
/// allocator intentionally does not exist (the kernel is meant to be as small and simple as possible, 
/// and any more complex functionality is implemented in user space drivers).
///
/// Using only page allocations also allows these structures to be easily mapped into any virtual address 
/// space (and to help facilitate this, many of the structures store physical addresses rather than virtual 
/// addresses).
///
/// Additionally, these structures are intentionally designed to be re-used as components of other 
/// structures.  As an example, a Mutex is really just a queue of waiting taskIDs with a unique 
/// header.
pub mod allocator;
pub mod node_allocator;
pub mod queue;