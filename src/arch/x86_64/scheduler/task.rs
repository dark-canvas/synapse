use crate::arch::x86_64::util::register_snapshot::RegisterSnapshot;
use crate::arch::x86_64::X86_PAGER;
use crate::page_based_list::PageBasedList;
use crate::Address;
use crate::arch::x86_64::scheduler::VirtualAddress;
use core::arch::asm;

unsafe fn kernel_task_entry() {
    asm!(
        "popq %rax",
        "call *%rax",
        options(att_syntax),
    )
    // setup user stack
    // iretd ? to return to ring3 code at the ring3 entry point
}

unsafe fn user_task_entry() {
    asm!(
        //"cli", // not strictly necessary
        // ring 3 stack segment (0x20 data segment + 0x03 privilege level)
        "pushq $0x23",
        // setup a stack pointer (what should this be? 0x800000000000 ? )
        //"movl $user_stack_top, %eax",
        //"pushl %eax",

        // stack base must be < 0x800000000000 (max address for lower canonical half is 0x7FFFFFFFFFFF)
        "pushq $0x0x7FFFFFFFF000",

        // Push the RFLAGS register (ensure interrupts are enabled)
        //"pushfq",
        //"popq %rax",
        //"orq $0x200, %rax",
        //"pushq %rax",
        "pushq $0x202",  // interrupt enable and RPL=3 

        // ring 3 code segment (0x18 code segment + 0x03 privilege level)
        "pushq $0x1b",

        // instruction pointer
        "movq $ring3_user_entry, %rax",
        "pushq %rax",

        // Clear/set segment registers for User Space (DS, ES, FS, GS)
        "movw $0x20, %ax",
        "movw %ax, %ds",
        "movw %ax, %es",
        "movw %ax, %fs",
        "movw %ax, %gs",

        // Execute the return to Ring 3
        "iretq",
        options(att_syntax),
    )
    // setup user stack
    // iretd ? to return to ring3 code at the ring3 entry point
}

// TODO: this is stale now that we have TaskMap... the page-based list should be of TaskHandle?
// TODO: should actually be a PageBasedQueue, anyway... FIFO, but also somehow with priorities.
// Must be, at most, 4096 - 8 to fit into page-based-list
#[allow(dead_code)]
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

#[allow(dead_code)]
impl Task {
    pub fn new_kernel_task(entry: Address, stack: VirtualAddress, stack_size: usize) -> Self {
        // REVISIT: this wouldn't have to be mut with rip and rflags were separate from registers..?
        let result = Task { 
            id: 0, 
            registers: RegisterSnapshot::default(),
            rip: entry, // TODO: need to wrap this in a handler that calls entry and cleans up upon return
            rflags: 0x202, // Interrupt Enable flag
            cr3: X86_PAGER.get().unwrap().get_kernel_cr3(),
            kernel_stack_pointer: stack + stack_size, //X86_PAGER.get().unwrap().allocate_stack(0x1000).unwrap() // Arbitrary stack size for now
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
