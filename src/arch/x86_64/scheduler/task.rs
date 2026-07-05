use crate::arch::x86_64::util::register_snapshot::RegisterSnapshot;
use crate::arch::x86_64::X86_PAGER;
use crate::page_based_list::PageBasedList;
use crate::Address;
use crate::arch::x86_64::scheduler::VirtualAddress;

// Must be, at most, 4096 - 8 to fit into page-based-list
pub struct Task {
    pub registers: RegisterSnapshot,
    pub rip: u64,
    pub rflags: u64,
    pub cr3: u64, // physical address and flags
    pub kernel_stack_pointer: VirtualAddress,
    pub id: u64, // need some way to lookup by id

    //parent: Option<&Task>, // or parent_id?
    // name?
    /*
    void* esp;
    void* esp0;
    void* cr3;
    thread_control_block* next;
    `uint8_t state;
    */
}

pub type TaskList = PageBasedList<Task>;

impl Task {
    pub fn new_kernel_task(entry: Address) -> Self {
        // REVISIT: this wouldn't have to be mut with rip and rflags were separate from registers..?
        let mut result = Task { 
            id: 0, 
            registers: RegisterSnapshot::default(),
            rip: entry,
            rflags: 0x202, // Interrupt Enable flag
            cr3: X86_PAGER.get().unwrap().get_kernel_cr3(),
            kernel_stack_pointer: VirtualAddress(0) //X86_PAGER.get().unwrap().allocate_stack(0x1000).unwrap() // Arbitrary stack size for now
        };
        //result.registers.rip = entry;
        //result.registers.rflags = 0x202; // Interrupt Enable flag
        // TODO: stack size passed in, and allocate stack from pager?
        //result.registers.rsp = KERNEL_START - 0x1000; // Arbitr
        result
    }

    /*
    pub fn new_user_task(entry: Address) -> Self {
        let result = Task { id: 0, registers: RegisterSnapshot::default() }
        result.registers.rip = entry.0;
        result.registers.rflags = 0x202; // Interrupt Enable flag
        // TODO: stack size passed in, and allocate stack from pager?
        //result.registers.rsp = KERNEL_START - 0x1000; // Arbitr
        result
    }
    */
}