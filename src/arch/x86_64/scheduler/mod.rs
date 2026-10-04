pub mod task;

use self::task::Task;
use self::task::TaskList;
use super::pager::VirtualAddress;
use super::GlobalData;
use crate::arch::x86_64::smp::cpu_state::CpuState;
use crate::errors::ErrCode;
//use crate::arch::x86_64::util::register_snapshot::RegisterSnapshot;
use crate::scheduler::Scheduler as SchedulerInterface;
use crate::scheduler::Task as TaskInterface;

use core::arch::global_asm;

unsafe extern "C" {
    /// Voluntarily called by a task to yield control to the scheduler, which will select the next task to run.
    pub fn yield_task_asm();

    // TODO: allow passing in the interupt stack frame for modification?
    // Or create an entirely different function for that?
}

// Inline assembly function directly into the binary
// It's important to note the convention that's used when calling this method:
//   The call instruction: It decrements the Stack Pointer (RSP) by 8 bytes, 
//   copies the address of the next sequential instruction (the return address) into that new memory location, 
//   and then jumps to the target function.
global_asm!(
    ".global yield_task_asm",
    ".text",
    "yield_task_asm:",
    // select the next task (call into rust for this)
    // restore the next task's context
    // iretq to the next task

    // save off the current task's context
    // see schduler::Task for the layout of the CURRENT_TASK struct
    "    push %rdi",
    // TODO: calculate CURRENT_TASK by querying the current taskID from the cpu state, 
    // multiply by the size of a CpuState, and add the offset
    "    xorq %rdi, %rdi",
    "    movl %gs:4, %edi",
    "    imulq $24760, %rdi, %rdi",  // immediate value is size_of::<Task>()
    "    push %rax",
    "    movq $0xFFFFFFF010000000, %rax",
    "    addq %rax, %rdi", // add the base of the task map
    "    pop %rax",
    //"    movq CURRENT_TASK(%rip), %rdi",
    // Task::registers (RegisterSnapshot) is the first field of Task, so we can just write to the start of the struct
    "    movq %rax, 0(%rdi)",   // RAX
    "    movq %rbx, 8(%rdi)",   // RBX
    "    movq %rcx, 16(%rdi)",  // RCX
    "    movq %rdx, 24(%rdi)",  // RDX
    "    movq %rsi, 32(%rdi)",  // RSI
    // RDI contains the CURRENT_TASK current, but the original value was pushed (above) and 
    // is handled later (below) -> 40(%rdi)
    "    movq %rbp, 48(%rdi)", // RBP 

    // save rsp into CURRENT_TASK, but remember to add 8 to skip over the push of 
    // rdi we did above (this effectively saves the return address of the yield_task call into the CURRENT_TASK struct)
    "    leaq 8(%rsp), %rax", // RSP 
    "    movq %rax, 56(%rdi)", // RSP 

    "    movq %r8,  64(%rdi)",   // R8
    "    movq %r9,  72(%rdi)",   // R9
    "    movq %r10, 80(%rdi)",  // R10
    "    movq %r11, 88(%rdi)",  // R11
    "    movq %r12, 96(%rdi)",  // R12
    "    movq %r13, 104(%rdi)",  // R13
    "    movq %r14, 112(%rdi)",  // R14
    "    movq %r15, 120(%rdi)",  // R15
    // After Task::registers comes rip and rflags
    // 0(rsp) == rdi currently
    
    "    movq 8(%rsp), %rax", // RIP is on the stack at the return address, which is at RBP (after the push above)
    "    movq %rax, 128(%rdi)", // RIP (need to get return address from stack)
    
    "    pushfq",
    "    popq %rax",
    "    movq %rax, 136(%rdi)", // RFLAGS
    // handle rdi
    "    pop %rax",
    "    movq %rax, 40(%rdi)", // RDI

    // Now switch to the next task (call into rust for this)
    // For now just use the same task
    "    xorq %rdi, %rdi",
    "    movl %gs:4, %edi",
    "    imulq $24760, %rdi, %rdi",  // immediate value is size_of::<Task>()
    "    push %rax",
    "    movq $0xFFFFFFF010000000, %rax",
    "    addq %rax, %rdi", // add the base of the task map
    "    pop %rax",

    //"    movq CURRENT_TASK(%rip), %rdi",
    "    movq 0(%rdi), %rax",   // RAX
    "    movq 8(%rdi), %rbx",   // RBX
    "    movq 16(%rdi), %rcx",  // RCX
    "    movq 24(%rdi), %rdx",  // RDX
    "    movq 32(%rdi), %rsi",  // RSI
    // RDI contains the CURRENT_TASK current, but the original value was pushed (above) and 
    // is handled later (below) -> 40(%rdi)
    "    movq 48(%rdi), %rbp", // RBP 
    "    movq 56(%rdi), %rsp", // RSP
    "    movq 64(%rdi), %r8",   // R8
    "    movq 72(%rdi), %r9",   // R9
    "    movq 80(%rdi), %r10",  // R10
    "    movq 88(%rdi), %r11",  // R11
    "    movq 96(%rdi), %r12",  // R12
    "    movq 104(%rdi), %r13",  // R13
    "    movq 112(%rdi), %r14",  // R14
    "    movq 120(%rdi), %r15",  // R15
    
    // What to do about RIP?  Currently we only have tasks switching by calling yield task, 
    // which means if/when we switch RSP to a new task, it'll be pointing to a spot where the 
    // return address is on the stack..
    // TODO:
    // pop the current return address
    // push the new return address on the stack


    //"    movq 8(%rsp), %rax", // RIP is on the stack at the return address, which is at RBP (after the push above)
    //"    movq %rax, 128(%rdi)", // RIP (need to get return address from stack)
    
    "    movq 136(%rdi), %rax", // RFLAGS
    "    push %rax",
    "    popfq",

    "    ret",
    options(att_syntax)
);

use crate::arch::x86_64::task_map::TASK_MAP_BASE_ADDRESS;
const _: () = {
    assert!(TASK_MAP_BASE_ADDRESS.0 == 0xFFFFFFF010000000, "Hardcoded task-map base needs updating in assembly");
    assert!(core::mem::size_of::<Task>() == 24760, "Hardcoded Task size needs updating in assembly");
};

// Safe wrapper around the assembly function
#[inline(always)]
pub fn yield_task() {
    unsafe { yield_task_asm() }
}

pub struct Scheduler {
    tasks: TaskList 
}

#[allow(dead_code)]
impl Scheduler {
    pub fn init() {
        // create a new scheduler per each CPU
        // set config.cpu_config.kernel_ap_entry to something that executes the scheduler
        // set config.cpu_config.ap_ready to true to unblock them

        // or does the above belong in a cpu init section?  Because there's more than just scheduling...
        // they also need to init the x2apic per each cpu
    }

    // TODO: needs the task map!
    pub fn new() -> Self {
        let mut scheduler = Scheduler {
            tasks: TaskList::new().expect("Failed to create task list"),
        };
        // Create a task for what is running right now (i.e., the kernel).  We don't need to initialize 
        // the registers here, as the first time we yield, we'll save off the current state of the kernel.
        // As multiple cores are supported this will likely need to change from a single static global 
        // to something per-cpu.
        let mut task_map = GlobalData::get().task_map.lock().unwrap();
        let task = task_map.new_task().unwrap();
        // initialize the task structure?  It'll be saved on task switch...
        //let id = task.get_id(); // TODO: this isn't initialized yet... and what should it be?
        let task_index = task_map.get_index(task).unwrap();
        let cpu_state = CpuState::get_local_cpu_state();
        cpu_state.current_tid = task_index as u32;
        println!("Set current task ID to {} for CPU {}", task_index, cpu_state.get_cpu_id());
        /*
        scheduler.add_task(
            Task {
                id: 0,
                registers: RegisterSnapshot::default(),
                rip: 0,
                rflags: 0x202, // Interrupt Enable flag
                cr3: get_kernel_cr3(),
                kernel_stack_pointer: VirtualAddress(0)
            }
        ).expect("Failed to add kernel task to task list");
        */
        scheduler
    }
}

impl SchedulerInterface for Scheduler {
    type Task = Task;

    // REVISTIT: this consumes task... is it okay?  Is it efficient (check the assembly)
    // TODO: I think it's not okay... the task comes form TaskMap and shouldn't ever be copied.
    // The lists should be of TaskID which can be used to index into the taskmap!
    fn add_task(&mut self, task: &Self::Task) -> Result<(), ErrCode> {
        self.tasks.add(task.get_id())
    }
}
