use super::Task;
use super::TaskList;
use super::TASK_MAP_BASE_ADDRESS;
use super::super::GlobalData;
use super::super::smp::CpuState;
use crate::errors::ErrCode;
use crate::pager::PAGER;
use crate::scheduler::Scheduler as SchedulerInterface;
use crate::scheduler::Task as TaskInterface;

use core::arch::global_asm;

const _: () = {
    assert!(TASK_MAP_BASE_ADDRESS.0 == 0xFFFFFFF010000000, "Hardcoded task-map base needs updating in assembly");
    assert!(core::mem::size_of::<Task>() == 24760, "Hardcoded Task size needs updating in assembly");
};

unsafe extern "C" {
    /// Voluntarily called by a task to yield control to the scheduler, which will select the next task to run.
    pub fn yield_task_asm();
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

    // Before saving off the context of the current task, we should check whether there's even another 
    // task that's waiting to be resumed.  To do that, we need to call into rust code, which is done via an 
    // extern "C" function, which requires the caller to save registers to ensure they aren't clobbered
    // (%rax, %rcx, %rdx, %rsi, %rdi, %r8, %r9, %r10, %r11, %rflags need to be saved).
    // Typically you would save them to the stack, but it makes more sense to just save them into the 
    // Task structure (if there is a waiting task, they need to be there anyway).

    "    push %rdi",
    "    pushfq",

    // calculate the current task pointer by querying the current taskID from the cpu state, 
    // multiply by the size of a Task, and add the task map base address to it.
    "    xorq %rdi, %rdi",
    "    movl %gs:8, %edi",
    "    push %rdi",                 // save off the current task ID for comparison later
    "    subl $1, %edi",             // subtract 1 to get the task index
    "    imulq $24760, %rdi, %rdi",  // immediate value is size_of::<Task>()
    "    push %rax",
    "    movq $0xFFFFFFF010000000, %rax",
    "    addq %rax, %rdi",           // add the base of the task map
    "    pop %rax",

    // save all the x86-64 ABI clobbered/scratch registers first (note rdi and rflags are pushed on the stack)
    "    movq %rax,  0(%rdi)",
    "    movq %rcx, 16(%rdi)",
    "    movq %rdx, 24(%rdi)",
    "    movq %rsi, 32(%rdi)",
    "    movq %r8,  64(%rdi)",
    "    movq %r9,  72(%rdi)",
    "    movq %r10, 80(%rdi)",
    "    movq %r11, 88(%rdi)",
    // original (task's) rdi is already on the stack, but we also need to save the current rdi, as it 
    // points to the current Task structure which (depending on the output of scheduler_get_next_task) 
    // we either need to keep saving state to, or restore the scratch registers from...
    "    push %rdi",

    // check if there's another task waiting to run.  If there is, the current task ID will be set.
    // If it's the same as it was before, then restore the previous registers and return to the 
    // same task, otherwise keep saving the context of the current task in preparation to switch 
    // to the next one
    "    call scheduler_get_next_task", // call into rust to get the next task to run
    "    movl %gs:8, %edi",             // current task id, possibly changed by the previous call
    "    cmpl %edi, 8(%rsp)",
    "    pop %rdi",                     // restore the previous Task struct
    "    jne 1f",

    // no task switch!!! Restore the registers we just saved, and return back to the task...
    "    addq $8, %rsp", // we don't need the previous task ID
    "    popf",
    "    movq  0(%rdi), %rax",
    "    movq 16(%rdi), %rcx",
    "    movq 24(%rdi), %rdx",
    "    movq 32(%rdi), %rsi",
    "    movq 64(%rdi), %r8",
    "    movq 72(%rdi), %r9",
    "    movq 80(%rdi), %r10",
    "    movq 88(%rdi), %r11",
    "    popq %rdi",
    "    ret",

    // if we reached this label, then there *is* a task to switch to, so keep saving the current task's state
    // note that the previous task ID is still at rsp; we don't need it anymore, so skip past it
    "1:",
    "    addq $8, %rsp",

    // we pushed the flags onto the stack above, so save it now...
    "    popq %rax",
    "    movq %rax, 136(%rdi)", // RFLAGS

    // save rsp into the prevoius task struct, but remember to add 8 to skip over the push of 
    // rdi we did to start this assembly.  By doing this, we save the stack position such that a pop will
    // return to the return address of this yield_task call)
    "    leaq 8(%rsp), %rax",  // RSP 
    "    movq %rax, 56(%rdi)", // RSP 

    "    movq %rbx, 8(%rdi)",
    "    movq %rbp, 48(%rdi)",

    "    movq %r12, 96(%rdi)",
    "    movq %r13, 104(%rdi)",
    "    movq %r14, 112(%rdi)",
    "    movq %r15, 120(%rdi)",
    
    "    movq 8(%rsp), %rax",   // RIP is on the stack at the return address, which is at RBP (after the push above)
    "    movq %rax, 128(%rdi)", // RIP (need to get return address from stack)
    
    // handle rdi
    "    pop %rax",
    "    movq %rax, 40(%rdi)", // RDI

    // This code (resolving a task_id to a Task*) is exactly the same as above.
    // Might be good to have a function (or maybe it can be emitted by a macro)
    "    xorq %rdi, %rdi",
    "    movl %gs:8, %edi",
    "    subl $1, %edi",
    "    imulq $24760, %rdi, %rdi",  // immediate value is size_of::<Task>()
    "    movq $0xFFFFFFF010000000, %rax",
    "    addq %rax, %rdi", // add the base of the task map
 
    "    movq 0(%rdi), %rax",
    "    movq 8(%rdi), %rbx",
    "    movq 16(%rdi), %rcx",
    "    movq 24(%rdi), %rdx",
    "    movq 32(%rdi), %rsi",
    "    movq 48(%rdi), %rbp",
    "    movq 56(%rdi), %rsp",
    "    movq 64(%rdi), %r8",
    "    movq 72(%rdi), %r9",
    "    movq 80(%rdi), %r10",
    "    movq 88(%rdi), %r11",
    "    movq 96(%rdi), %r12",
    "    movq 104(%rdi), %r13",
    "    movq 112(%rdi), %r14",
    "    movq 120(%rdi), %r15",
   
    "    movq 136(%rdi), %rax", // RFLAGS
    "    push %rax", 
    "    popfq",

    "    ret",
    options(att_syntax)
);

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
