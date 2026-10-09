pub mod task;

use self::task::Task;
use self::task::TaskList;
use super::pager::VirtualAddress;
use super::GlobalData;
use crate::arch::x86_64::smp::cpu_state::CpuState;
use crate::errors::ErrCode;
use crate::pager::PAGER;
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

    "    push %rdi",
    // calculate the current task pointer by querying the current taskID from the cpu state, 
    // multiply by the size of a Task, and add the task map base address to it.
    "    xorq %rdi, %rdi",
    "    movl %gs:8, %edi",  // OR load the whole thing and and with 0xFFFFFFFF to get the loewr part (task index + 1)
    "    subl $1, %edi",  // subtract 1 to get the task index
    "    imulq $24760, %rdi, %rdi",  // immediate value is size_of::<Task>()
    "    push %rax",
    "    movq $0xFFFFFFF010000000, %rax",
    "    addq %rax, %rdi", // add the base of the task map
    "    pop %rax",

    // Task::registers (RegisterSnapshot) is the first field of Task, so we can just write to the start of the struct
    "    movq %rax, 0(%rdi)",   // RAX
    "    movq %rbx, 8(%rdi)",   // RBX
    "    movq %rcx, 16(%rdi)",  // RCX
    "    movq %rdx, 24(%rdi)",  // RDX
    "    movq %rsi, 32(%rdi)",  // RSI
    // RDI contains the current task pointer, but the original value was pushed (above) and 
    // is handled later (below) -> 40(%rdi)
    "    movq %rbp, 48(%rdi)", // RBP 

    // save rsp into the current task, but remember to add 8 to skip over the push of 
    // rdi we did above.  By doing this, we save the stack position such that a pop will
    // return to the return address of this yield_task call)
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
    
    "    movq 8(%rsp), %rax", // RIP is on the stack at the return address, which is at RBP (after the push above)
    "    movq %rax, 128(%rdi)", // RIP (need to get return address from stack)
    
    "    pushfq",
    "    popq %rax",
    "    movq %rax, 136(%rdi)", // RFLAGS
    // handle rdi
    "    pop %rax",
    "    movq %rax, 40(%rdi)", // RDI

    "    call scheduler_get_next_task", // call into rust to get the next task to run

    // This code (resolving a task_id to a Task*) is exactly the same as above.
    // Might be good to have a function (or maybe it can be emitted by a macro)
    "    xorq %rdi, %rdi",
    "    movl %gs:8, %edi",
    "    subl $1, %edi",
    "    imulq $24760, %rdi, %rdi",  // immediate value is size_of::<Task>()
    "    movq $0xFFFFFFF010000000, %rax",
    "    addq %rax, %rdi", // add the base of the task map
 
    "    movq 0(%rdi), %rax",   // RAX
    "    movq 8(%rdi), %rbx",   // RBX
    "    movq 16(%rdi), %rcx",  // RCX
    "    movq 24(%rdi), %rdx",  // RDX
    "    movq 32(%rdi), %rsi",  // RSI
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


#[unsafe(no_mangle)]
pub extern "C" fn scheduler_get_next_task() {
    let cpu_state = CpuState::get_local_cpu_state();
    let scheduler = &mut cpu_state.scheduler;
    if let Ok(next_task_id) = scheduler.tasks.dequeue() {
        // Add back to the end (TODO: if still able to run)
        scheduler.tasks.enqueue(cpu_state.current_tid).unwrap();
        cpu_state.current_tid = next_task_id;
        return;
    }
}

#[used]
static SCHEDULER_GET_NEXT_TASK_REF: unsafe extern "C" fn() = scheduler_get_next_task;

//#[derive(Default)]
pub struct Scheduler {
    tasks: TaskList,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            tasks: TaskList::new(crate::pager::get_pager()),
        }
    }
}

#[allow(dead_code)]
impl Scheduler {
    // REVISIT: pass in the taskmap?  Or continue to get it from GlobalData?
    pub fn new() -> Self {
        let scheduler = Scheduler {
            tasks: TaskList::new(*PAGER.borrow()),
        };
        // Create a task for what is running right now (i.e., the kernel).  We don't need to initialize 
        // the registers here, as the first time we yield, we'll save off the current state of the kernel.
        let mut task_map = GlobalData::get().task_map.lock().unwrap();
        let task = task_map.new_task().unwrap();
        let cpu_state = CpuState::get_local_cpu_state();
        cpu_state.current_tid = task.get_id();
        println!("Set current task ID to {} for CPU {}", cpu_state.current_tid, cpu_state.get_cpu_id());

        scheduler
    }

    pub fn new_task<F>(&mut self, f: F) -> Result<(), ErrCode>
    where
        F: FnOnce() + Send + 'static,
    {
        let mut task_map = GlobalData::get().task_map.lock().unwrap();
        let task: &mut Task = task_map.new_task().unwrap();
        task.initialize(f);
        self.tasks.enqueue(task.get_id()).unwrap();
        Ok(())
    }
}

impl SchedulerInterface for Scheduler {
    type Task = Task;

    // REVISTIT: this consumes task... is it okay?  Is it efficient (check the assembly)
    // TODO: I think it's not okay... the task comes form TaskMap and shouldn't ever be copied.
    // The lists should be of TaskID which can be used to index into the taskmap!
    fn add_task(&mut self, task: &Self::Task) -> Result<(), ErrCode> {
        self.tasks.enqueue(task.get_id()).unwrap();
        Ok(())
    }
}
