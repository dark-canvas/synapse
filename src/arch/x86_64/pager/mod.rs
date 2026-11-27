//! Pager implementation
//!
//! The kernel is mapped to 0xFFFFFF8000000000 (last element in pl4 table)
//!   p4 index = 511
//!   p3 index = 0
//!   p2 index = 0
//!   p1 index = 0
//! The stack base is also at 0xFFFFFF8000000000, but expands downwards
//! 
//! Physical memory is "identity" mapped into pl4[510] == 0xFFFFFF0000000000
//! Can just copy pl4[0] to pl4[510]?
//!
//! This results in support for up to 512GB of physical memory, which equates to:
//!   - 134217728 4kb pages (2^27) == 1073741824 (1gb) required for the page stack
//!   - 262144 2mb pages (2^18)    == 2097152 (2mb) required for the page stack
//!   - 512 1gb pages (2^9)        == 4096 requires for the page stack
//!
//! Which can be mapped to the top of the virtual address space (above the kernel)
//!   - 0xFFFFFFD000000000 - 0xFFFFFFE000000000 -> 4kb page management (prefer to only allocate these for page tables?)
//!      - 0xFFFFFFD000000000 - 0xFFFFFFD040000000 -> 4kb page stack (512GB / 4KB * 8 bytes per entry) == 1gb (allocate multiple 2mb pages for this?)
//!      - 0xFFFFFFD040000000 - 0xFFFFFFD040100000 -> 2mb page aggregator (512GB / 2MB * sizeof(PageBucket) (4)) == 1mb (could allocate a 2mb page for this?)
//!   - 0xFFFFFFE000000000 - 0xFFFFFFE000200000 -> 2mb page management
//!      - 0xFFFFFFE000000000 - 0xFFFFFFE000200000 -> 2mb page stack (512GB / 2MB * 8 bytes per entry) == 2m (allocate a 2mb page for this?)
//!      - 0xFFFFFFE000200000 - 0xFFFFFFE000201000 -> 1gb page aggregator (512GB / 1GB * sizeof(PageBucket) (4)) == 2kb (rounded to 1 page)
//!   - 0xFFFFFFE000201000 - 0xFFFFFFE000202000 -> 1gb page management
//!      - 0xFFFFFFE000201000 - 0xFFFFFFE000202000 -> 1gb page stack (512GB / 1GB * 8 bytes per entry) == 4kb
//!      - 0xFFFFFFE000202000 - 0xFFFFFFE000203000 -> 512gb page aggregator is really just a single PageBucket
//!
//! Note that, for symmetry, I've createad a 512GB page aggregator (eg. 1GB * ADDRESSES_PER_PAGE).
//! There isn't a notion of a 512GB page in the 4 level paging that is used by this kernel, however, 
//! 5 level paging does exist, and does introduce the notion of a 512GB page, so this aggregator allows 
//! for more easily adapting to 5 level paging in the future.
//! However, the current intent of this structure isn't to check whether we can aggregate 512 adjacent 
//! 1GB pages and return them to a larger page stack, but instead just a simple tracking of 
//! available/allocated pages (and it simplifies the code).
//! The simplification breaks if the host platform actually has >= 512GB of memory, as the aggregator 
//! will try to consolodate adjacent pages into a 512GB page if it can, but this is an intentional 
//! design decision/tradeoff -- the current kernel does not support more than 512GB of physical memory.
//! Supporting >= 512GB of physical memory will likely also involve supporting 5 level paging.

pub mod page_stack;
pub mod page_iterator;
pub mod address_aggregator;

mod constants;
pub use self::constants::{
    PAGE_MASK_1GB, PAGE_MASK_2MB, PAGE_MASK_4KB, PAGE_OFFSET_MASK_1GB,
    PAGE_OFFSET_MASK_2MB, PAGE_OFFSET_MASK_4KB, PAGE_SIZE_1GB, PAGE_SIZE_2MB,
    PAGE_SIZE_4KB, PAGER_MAX_SUPPORTED_MEMORY,
};
pub(crate) use self::constants::PHYSICAL_OFFSET;
use self::constants::{
    PAGE_AGGREGATOR_1GB_BASE, PAGE_AGGREGATOR_2MB_BASE, PAGE_AGGREGATOR_512GB_BASE,
    PAGE_STACK_1GB_BASE, PAGE_STACK_1GB_MAX_PAGES, PAGE_STACK_2MB_BASE,
    PAGE_STACK_2MB_MAX_PAGES, PAGE_STACK_4KB_BASE, PAGE_STACK_4KB_MAX_PAGES,
};

mod page_type;
pub use self::page_type::PageType;
mod sized_page;
pub use self::sized_page::SizedPage;

mod util;
pub use self::util::{
    get_kernel_cr3, get_pl4_table, get_entry, invalidate_page, is_1gb_aligned,
    is_2mb_aligned, is_4kb_aligned, next_1gb_page, next_2mb_page,
    next_4kb_page, pages_required, physical_mirror, set_mmio,
};
use self::util::entry_to_table;

mod run_time_tests;
pub use self::run_time_tests::run_time_tests;

mod pager;
pub use self::pager::Pager;
