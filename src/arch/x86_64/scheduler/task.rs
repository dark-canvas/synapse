use x86_64::registers::segmentation::CS;
use x86_64::registers::segmentation::SS;
use x86_64::registers::segmentation::Segment;

use crate::arch::x86_64::scheduler::TaskHandle;
use crate::arch::x86_64::util::register_snapshot::RegisterSnapshot;
use crate::arch::x86_64::X86_PAGER;
use crate::page_based::Queue;
use crate::pager::Pager;
use crate::pager::VirtualAddress;
use crate::scheduler::Task as TaskInterface;
use crate::scheduler::TaskHandle as TaskHandleInterface;

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
    pub cs: u16,
    pub ss: u16,
    pub id: TaskHandle,
    pub kernel_stack: [u8; 16*1024],
    pub io_bitmap: Option<&'static [u8; 8193]>,
}

pub type TaskList = Queue<&'static dyn Pager, TaskHandle>;

#[allow(dead_code)]
impl Task {
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
        self.cs = CS::get_reg().0;
        self.ss = SS::get_reg().0;

        println!("Task initialized with RIP: {:#x}, RSP: {:#x}, RDI: {:#x}, RFLAGS: {:#x}, CR3: {:#x}", 
            self.rip, self.registers.rsp, self.registers.rdi, self.rflags, self.cr3);

        // id, stack and io bitmap are initialized when return from the task_map
    }
}

impl TaskInterface for Task {
    type TaskHandle = TaskHandle;

    fn get_handle(&self) -> TaskHandle {
        self.id
    }
}
