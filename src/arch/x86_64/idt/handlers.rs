use x86_64::structures::idt::InterruptStackFrame;
use x86_64::structures::idt::InterruptDescriptorTable;
use x86_64::structures::idt::HandlerFunc;
use x86_64::structures::idt::PageFaultErrorCode;
use core::arch::asm;

use crate::KERNEL_START;

pub trait InterruptMetaData {
    const INTERRUPT_NUMBER: u8;
    const NAME: &'static str;
}

#[derive(Debug, Default)]
struct SystemSnapshot {
    rax: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    rsp: u64,
    r8:  u64,
    r9:  u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
}

impl SystemSnapshot {
    fn dump_registers(&self) {
        println!("RAX: {:#018x}    R8:  {:#018x}", self.rax, self.r8);
        println!("RBX: {:#018x}    R9:  {:#018x}", self.rbx, self.r9);
        println!("RCX: {:#018x}    R10: {:#018x}", self.rcx, self.r10);
        println!("RDX: {:#018x}    R11: {:#018x}", self.rdx, self.r11);
        println!("RSI: {:#018x}    R12: {:#018x}", self.rsi, self.r12);
        println!("RDI: {:#018x}    R13: {:#018x}", self.rdi, self.r13);
        println!("RBP: {:#018x}    R14: {:#018x}", self.rbp, self.r14);
        println!("RSP: {:#018x}    R15: {:#018x}", self.rsp, self.r15);
    }

    fn dump_stack(&self) {
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

macro_rules! get_system_snapshot {
    () => {{
        let mut snapshot = SystemSnapshot::default();
        unsafe {
            asm!(
                "nop",
                out("rax") snapshot.rax, out("rcx") snapshot.rcx, out("rdx") snapshot.rdx,
                out("rsi") snapshot.rsi, out("rdi") snapshot.rdi,
                out("r8")  snapshot.r8,  out("r9")  snapshot.r9,  out("r10") snapshot.r10, out("r11") snapshot.r11,
                out("r12") snapshot.r12, out("r13") snapshot.r13, out("r14") snapshot.r14, out("r15") snapshot.r15,
            );
            // rbp and rsp are disallowed as operands for inline assembly, so we can easily query them using the 
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
                out(reg) snapshot.rbp,
                out(reg) snapshot.rsp,
                out(reg) snapshot.rbx,
            );
        }
        snapshot
    }};
}

pub extern "x86-interrupt" fn default_handler_x86<T: InterruptMetaData>(
    stack_frame: InterruptStackFrame)
{
    let sys = get_system_snapshot!();
    
    println!("EXCEPTION #{} - {}:\n{:#?}", T::INTERRUPT_NUMBER, T::NAME, stack_frame);
    sys.dump_registers();
    sys.dump_stack();
}

pub extern "x86-interrupt" fn diverging_handler_x86<T: InterruptMetaData>(
    stack_frame: InterruptStackFrame) -> !
{
    let sys = get_system_snapshot!();
    println!("EXCEPTION #{} - {}:\n{:#?}", T::INTERRUPT_NUMBER, T::NAME, stack_frame);
    sys.dump_registers();
    sys.dump_stack();

    loop {
        x86_64::instructions::hlt();
    }
}

pub extern "x86-interrupt" fn default_handler_with_error_code_x86<T: InterruptMetaData>(
    stack_frame: InterruptStackFrame, error_code: u64)
{
    let sys = get_system_snapshot!();
    println!("EXCEPTION #{} - {}:\n{:#?}", T::INTERRUPT_NUMBER, T::NAME, stack_frame);
    println!("Error Code: {:?}", error_code);
    sys.dump_registers();
    sys.dump_stack();
}

pub extern "x86-interrupt" fn diverging_handler_with_error_code_x86<T: InterruptMetaData>(
    stack_frame: InterruptStackFrame, error_code: u64) -> !
{
    let sys = get_system_snapshot!();
    println!("EXCEPTION #{} - {}:\n{:#?}", T::INTERRUPT_NUMBER, T::NAME, stack_frame);
    println!("Error Code: {:?}", error_code);
    sys.dump_registers();
    sys.dump_stack();

    loop {
        x86_64::instructions::hlt();
    }
}

pub extern "x86-interrupt" fn page_fault_handler<T: InterruptMetaData>(
    stack_frame: InterruptStackFrame, 
    error_code: PageFaultErrorCode)
{
    let sys = get_system_snapshot!();

    println!("EXCEPTION #{} - {}:\n{:#?}", T::INTERRUPT_NUMBER, T::NAME, stack_frame);
    println!("Accessed Address: {:?}", x86_64::registers::control::Cr2::read());
    println!("Error Code: {:?}", error_code);

    sys.dump_registers();
    sys.dump_stack();
}

pub struct DivideByZeroMetaData;
pub struct DebugMetaData;
pub struct NonMaskableInterruptMetaData;
pub struct BreakpointMetaData;
pub struct OverflowMetaData;
pub struct BoundRangeExceededMetaData;
pub struct InvalidOpcodeMetaData;
pub struct DeviceNotAvailableMetaData;
pub struct DoubleFaultMetaData;
pub struct InvalidTssMetaData;
pub struct SegmentNotPresentMetaData;
pub struct StackSegmentFaultMetaData;
pub struct GeneralProtectionFaultMetaData;
pub struct PageFaultMetaData;
pub struct X87FloatingPointMetaData;
pub struct AlignmentCheckMetaData;
pub struct MachineCheckMetaData;
pub struct SimdFloatingPointMetaData;
pub struct VirtualizationMetaData;
pub struct CpProtectionExceptionMetaData;
pub struct HVInjectionExceptionMetaData;
pub struct VMMCommunicationExceptionMetaData;
pub struct SecurityExceptionMetaData;

impl InterruptMetaData for DivideByZeroMetaData {
    const INTERRUPT_NUMBER: u8 = 0;
    const NAME: &'static str = "Divide By Zero";
}

impl InterruptMetaData for DebugMetaData {
    const INTERRUPT_NUMBER: u8 = 1;
    const NAME: &'static str = "Debug";
}

impl InterruptMetaData for NonMaskableInterruptMetaData {
    const INTERRUPT_NUMBER: u8 = 2;
    const NAME: &'static str = "Non Maskable Interrupt";
}

impl InterruptMetaData for BreakpointMetaData {
    const INTERRUPT_NUMBER: u8 = 3;
    const NAME: &'static str = "Breakpoint";
}

impl InterruptMetaData for OverflowMetaData {
    const INTERRUPT_NUMBER: u8 = 4;
    const NAME: &'static str = "Overflow";
}

impl InterruptMetaData for BoundRangeExceededMetaData {
    const INTERRUPT_NUMBER: u8 = 5;
    const NAME: &'static str = "Bound Range Exceeded";
}

impl InterruptMetaData for InvalidOpcodeMetaData {
    const INTERRUPT_NUMBER: u8 = 6;
    const NAME: &'static str = "Invalid Opcode";
}

impl InterruptMetaData for DeviceNotAvailableMetaData {
    const INTERRUPT_NUMBER: u8 = 7;
    const NAME: &'static str = "Device Not Available";
}

impl InterruptMetaData for DoubleFaultMetaData {
    const INTERRUPT_NUMBER: u8 = 8;
    const NAME: &'static str = "Double Fault";
}

impl InterruptMetaData for InvalidTssMetaData {
    const INTERRUPT_NUMBER: u8 = 10;
    const NAME: &'static str = "Invalid TSS";
}

impl InterruptMetaData for SegmentNotPresentMetaData {
    const INTERRUPT_NUMBER: u8 = 11;
    const NAME: &'static str = "Segment Not Present";
}

impl InterruptMetaData for StackSegmentFaultMetaData {
    const INTERRUPT_NUMBER: u8 = 12;
    const NAME: &'static str = "Stack Segment Fault";
}

impl InterruptMetaData for GeneralProtectionFaultMetaData {
    const INTERRUPT_NUMBER: u8 = 13;
    const NAME: &'static str = "General Protection Fault";
}

impl InterruptMetaData for PageFaultMetaData {
    const INTERRUPT_NUMBER: u8 = 14;
    const NAME: &'static str = "Page Fault";
}

impl InterruptMetaData for X87FloatingPointMetaData {
    const INTERRUPT_NUMBER: u8 = 16;
    const NAME: &'static str = "X87 Floating Point";
}

impl InterruptMetaData for AlignmentCheckMetaData {
    const INTERRUPT_NUMBER: u8 = 17;
    const NAME: &'static str = "Alignment Check";
}

impl InterruptMetaData for MachineCheckMetaData {
    const INTERRUPT_NUMBER: u8 = 18;
    const NAME: &'static str = "Machine Check";
}

impl InterruptMetaData for SimdFloatingPointMetaData {
    const INTERRUPT_NUMBER: u8 = 19;
    const NAME: &'static str = "SIMD Floating Point";
}

impl InterruptMetaData for VirtualizationMetaData {
    const INTERRUPT_NUMBER: u8 = 20;
    const NAME: &'static str = "Virtualization";
}

impl InterruptMetaData for CpProtectionExceptionMetaData {
    const INTERRUPT_NUMBER: u8 = 21;
    const NAME: &'static str = "CPU Protection Exception";
}

impl InterruptMetaData for HVInjectionExceptionMetaData {
    const INTERRUPT_NUMBER: u8 = 22;
    const NAME: &'static str = "HV Injection Exception";
}

impl InterruptMetaData for VMMCommunicationExceptionMetaData {
    const INTERRUPT_NUMBER: u8 = 23;
    const NAME: &'static str = "VMM Communication Exception";
}

impl InterruptMetaData for SecurityExceptionMetaData {
    const INTERRUPT_NUMBER: u8 = 28;
    const NAME: &'static str = "Security Exception";
}
