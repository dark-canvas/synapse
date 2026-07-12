use core::arch::asm;
use crate::KERNEL_START;

#[repr(C)]
#[derive(Debug, Default)]
pub struct RegisterSnapshot {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8:  u64,
    pub r9:  u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
}

impl RegisterSnapshot {
    pub const fn default() -> Self {
        RegisterSnapshot {
            rax: 0, rbx: 0, rcx: 0, rdx: 0,
            rsi: 0, rdi: 0, rbp: 0, rsp: 0,
            r8: 0, r9: 0, r10: 0, r11: 0,
            r12: 0, r13: 0, r14: 0, r15: 0,
            //rip: 0, rflags: 0,
        }
    }

    pub fn dump_registers(&self) {
        println!("RAX: {:#018x}    R8:  {:#018x}", self.rax, self.r8);
        println!("RBX: {:#018x}    R9:  {:#018x}", self.rbx, self.r9);
        println!("RCX: {:#018x}    R10: {:#018x}", self.rcx, self.r10);
        println!("RDX: {:#018x}    R11: {:#018x}", self.rdx, self.r11);
        println!("RSI: {:#018x}    R12: {:#018x}", self.rsi, self.r12);
        println!("RDI: {:#018x}    R13: {:#018x}", self.rdi, self.r13);
        println!("RBP: {:#018x}    R14: {:#018x}", self.rbp, self.r14);
        println!("RSP: {:#018x}    R15: {:#018x}", self.rsp, self.r15);
    }

    pub fn dump_stack(&self) {
        let mut stack_len = KERNEL_START - self.rsp;
        println!("Stack Length: {} bytes", stack_len);
        stack_len /= 8;
        for i in 0..stack_len {
            let addr = self.rsp + i*8;
            let value = unsafe { *(addr as *const u64) };
            println!("Stack[{}] (0x{:016x}): 0x{:016x}", i, addr, value);
        }
    }
}

#[macro_export]
macro_rules! get_register_snapshot {
    () => {{
        let mut snapshot = RegisterSnapshot::default();
        unsafe {
            asm!(
                "nop",
                out("rax") snapshot.rax, out("rcx") snapshot.rcx, out("rdx") snapshot.rdx,
                out("rsi") snapshot.rsi, out("rdi") snapshot.rdi,
                out("r8")  snapshot.r8,  out("r9")  snapshot.r9,  out("r10") snapshot.r10, out("r11") snapshot.r11,
                out("r12") snapshot.r12, out("r13") snapshot.r13, out("r14") snapshot.r14, out("r15") snapshot.r15,
            );
            // rbp and rsp are disallowed as operands for inline assembly, so we can't easily query them using the 
            // above pattern (the compiler simply wont allow it).
            // Also, rbx is used by the LLVM-based backend (apparently often as a base pointer for PIC) and so 
            // using it as an output register (even if you don't modify the register!) is also not allowed.  But we 
            // can't copy it to another register (above) as that would invalidate the other register... so... 
            // instead, after getting what we can easily get above, we let the LLMV decide how to get the remaining 
            // registers.
            // At this point it doesn't matter if it clobbers another register, since we've already saved them 
            // off.  I fully expect the compiler just emits a "mov snapshot.rbx, rbx" anyway, but it apparently 
            // doesn't realize that's all I was attempting to do above as well.
            asm!(
                "mov {0}, rbp",
                "mov {1}, rsp",
                "mov {2}, rbx",
                //"pushfq",
                // "pop {3}",
                out(reg) snapshot.rbp,
                out(reg) snapshot.rsp,
                out(reg) snapshot.rbx,
                //out(reg) snapshot.rflags,
            );
        }
        snapshot
    }};
}

#[macro_export]
macro_rules! restore_register_snapshot {
    ($snapshot:expr) => {{
        let snapshot: &RegisterSnapshot = $snapshot;
        unsafe {
            asm!(
                "mov r15, {0}",
                "mov r14, {1}",
                "mov r13, {2}",
                "mov r12, {3}", 
                "mov r11, {4}",
                "mov r10, {5}",
                "mov r9, {6}",
                "mov r8, {7}",
                "mov rbp, {8}",
                // RDI would go here, but has to be done at the end
                "mov rsi, {9}",
                "mov rdx, {10}",
                "mov rcx, {11}",
                "mov rbx, {12}",
                "mov rax, {13}",
                //"push {14}",
                //"popfq",
                //"mov rdi, {15}",
                in(reg) snapshot.r15,
                in(reg) snapshot.r14,
                in(reg) snapshot.r13,
                in(reg) snapshot.r12,
                in(reg) snapshot.r11,
                in(reg) snapshot.r10,
                in(reg) snapshot.r9,
                in(reg) snapshot.r8,
                in(reg) snapshot.rbp,
                in(reg) snapshot.rsi,
                in(reg) snapshot.rdx,
                in(reg) snapshot.rcx,
                in(reg) snapshot.rbx,
                in(reg) snapshot.rax,
                //in(reg) snapshot.rflags,
                //in(reg) snapshot.rdi,
            );
            asm!(
                "push {0}",
                "popfq",
                in(reg) snapshot.rflags,
            );
            asm!(
                "mov rdi, {0}",
                in(reg) snapshot.rdi,
            );
            // Note that we have to restore rdi at the end, since it's used as the source for the other registers above.
        }
    }};
}