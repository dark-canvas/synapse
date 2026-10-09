use crate::arch::x86_64::util::register_snapshot::RegisterSnapshot;
use crate::arch::x86_64::X86_PAGER;
use crate::page_based::queue::Queue;
use crate::arch::x86_64::scheduler::VirtualAddress;
use crate::pager::Pager;
use crate::scheduler::Task as TaskInterface;
use crate::scheduler::TaskId;

// C function (therefore can get a function pointer to it) that acts as a trampoline 
// to call a Rust closure. The closure is passed in as a raw pointer to the closure data, 
// which is then reconstructed and called.
extern "C" fn closure_trampoline<F>(closure_raw_ptr: *mut ()) 
where
    F: FnOnce(),
{
    unsafe {
        // Reconstruct ownership of the closure from the pointer and immediately run it.
        // Reading it out of the raw pointer consumes it, preventing leaks.
        let closure = core::ptr::read(closure_raw_ptr as *mut F);
        closure();
    }
}

// TODO: just round up to 32k?
#[allow(dead_code)]
#[repr(C)]
pub struct Task {
    pub registers: RegisterSnapshot,     // set in yield_task assembly, don't move
    pub rip: u64,                        // set in yeild task assembly, don't move
    pub rflags: u64,                     // set in yield task assembly, don't move
    pub cr3: u64, // physical address and flags
    pub user_gs_base: VirtualAddress,
    pub kernel_stack_pointer: VirtualAddress, // don't think this is used...
    pub id: u64, // need some way to lookup by id
    pub kernel_stack: [u8; 16*1024],
    pub io_bitmap: [u8; 8193],
}

pub type TaskList = Queue<&'static dyn Pager, TaskId>;

// TODO: move to task_id.rs, make only public to the module (will require moving task_map into module)
pub fn get_task_index_from_id(task_id: TaskId) -> usize {
    (task_id & 0xFFFFFFFF) as usize - 1
}

pub fn get_task_generation_from_id(task_id: TaskId) -> u32 {
    (task_id >> 32) as u32
}

pub fn create_task_id(index: usize, generation: u32) -> TaskId {
    ((generation as u64) << 32) | (index as u64) + 1
}

#[allow(dead_code)]
impl Task {
    /* 
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
    */

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
    pub fn initialize<F>(&mut self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        // Get the data pointer (the environment context) from the closure
        let data_ptr = &f as *const F as *mut ();
        self.registers.rdi = data_ptr as u64; // Pass the closure data pointer in RDI (first argument)
    
        // Get the code pointer (closure_trampoline is a C wrapper that will call the closure) 
        let code_ptr = closure_trampoline::<F> as *const ();
        
        // Set the instruction pointer to the trampoline function (RIP is set just for readability, 
        // it's the write to the stack that actually sets the instruction pointer for when this task is resumed)
        self.rip = code_ptr as u64; // Set the instruction pointer to the trampoline function
        let offset = self.kernel_stack.len() - 8; // Reserve space for the return address
        self.kernel_stack[offset..].copy_from_slice(&self.rip.to_le_bytes());
        self.registers.rsp = self.kernel_stack.as_ptr() as u64 + offset as u64;
        
        self.rflags = 0x202; // Interrupt Enable flag
        self.cr3 = X86_PAGER.get().unwrap().get_kernel_cr3();

        println!("Task initialized with RIP: {:#x}, RSP: {:#x}, RDI: {:#x}, RFLAGS: {:#x}, CR3: {:#x}", 
            self.rip, self.registers.rsp, self.registers.rdi, self.rflags, self.cr3);

        // id, stack and io bitmap are initialized when return from the task_map
    }
}

impl TaskInterface for Task {
    fn get_id(&self) -> TaskId {
        self.id
    }
}
