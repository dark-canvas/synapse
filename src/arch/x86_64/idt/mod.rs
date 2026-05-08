use x86_64::structures::idt::InterruptStackFrame;
use x86_64::structures::idt::InterruptDescriptorTable;
use x86_64::structures::idt::HandlerFunc;
use x86_64::structures::idt::PageFaultErrorCode;
use lazy_static::lazy_static;

trait InterruptMetaData {
    const INTERRUPT_NUMBER: u8;
    const NAME: &'static str;
}

struct DivideByZeroMetaData;
struct DebugMetaData;
struct NonMaskableInterruptMetaData;
struct BreakpointMetaData;
struct OverflowMetaData;
struct BoundRangeExceededMetaData;
struct InvalidOpcodeMetaData;
struct DeviceNotAvailableMetaData;
struct DoubleFaultMetaData;
struct InvalidTssMetaData;
struct SegmentNotPresentMetaData;
struct StackSegmentFaultMetaData;
struct GeneralProtectionFaultMetaData;
struct PageFaultMetaData;
struct X87FloatingPointMetaData;
struct AlignmentCheckMetaData;
struct MachineCheckMetaData;
struct SimdFloatingPointMetaData;
struct VirtualizationMetaData;
struct CpProtectionExceptionMetaData;
struct HVInjectionExceptionMetaData;
struct VMMCommunicationExceptionMetaData;
struct SecurityExceptionMetaData;

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

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.divide_error.set_handler_fn(default_handler_x86::<DivideByZeroMetaData>);
        idt.debug.set_handler_fn(default_handler_x86::<DebugMetaData>);
        idt.non_maskable_interrupt.set_handler_fn(default_handler_x86::<NonMaskableInterruptMetaData>);
        idt.breakpoint.set_handler_fn(default_handler_x86::<BreakpointMetaData>);
        idt.overflow.set_handler_fn(default_handler_x86::<OverflowMetaData>);
        idt.bound_range_exceeded.set_handler_fn(default_handler_x86::<BoundRangeExceededMetaData>);
        idt.invalid_opcode.set_handler_fn(default_handler_x86::<InvalidOpcodeMetaData>);
        idt.device_not_available.set_handler_fn(default_handler_x86::<DeviceNotAvailableMetaData>);
        idt.double_fault.set_handler_fn(diverging_handler_with_error_code_x86::<DoubleFaultMetaData>);
        idt.invalid_tss.set_handler_fn(default_handler_with_error_code_x86::<InvalidTssMetaData>);
        idt.segment_not_present.set_handler_fn(default_handler_with_error_code_x86::<SegmentNotPresentMetaData>);
        idt.stack_segment_fault.set_handler_fn(default_handler_with_error_code_x86::<StackSegmentFaultMetaData>);
        idt.general_protection_fault.set_handler_fn(default_handler_with_error_code_x86::<GeneralProtectionFaultMetaData>);
        idt.page_fault.set_handler_fn(page_fault_handler::<PageFaultMetaData>);
        idt.x87_floating_point.set_handler_fn(default_handler_x86::<X87FloatingPointMetaData>);
        idt.alignment_check.set_handler_fn(default_handler_with_error_code_x86::<AlignmentCheckMetaData>);
        idt.machine_check.set_handler_fn(diverging_handler_x86::<MachineCheckMetaData>);
        idt.simd_floating_point.set_handler_fn(default_handler_x86::<SimdFloatingPointMetaData>);
        idt.virtualization.set_handler_fn(default_handler_x86::<VirtualizationMetaData>);
        idt.cp_protection_exception.set_handler_fn(default_handler_with_error_code_x86::<CpProtectionExceptionMetaData>);
        idt.hv_injection_exception.set_handler_fn(default_handler_x86::<HVInjectionExceptionMetaData>);
        idt.vmm_communication_exception.set_handler_fn(default_handler_with_error_code_x86::<VMMCommunicationExceptionMetaData>);
        idt.security_exception.set_handler_fn(default_handler_with_error_code_x86::<SecurityExceptionMetaData>);
        idt
    };
}

extern "x86-interrupt" fn default_handler_x86<T: InterruptMetaData>(
    stack_frame: InterruptStackFrame)
{
    println!("EXCEPTION #{} - {}:\n{:#?}", T::INTERRUPT_NUMBER, T::NAME, stack_frame);
}

extern "x86-interrupt" fn diverging_handler_x86<T: InterruptMetaData>(
    stack_frame: InterruptStackFrame) -> !
{
    println!("EXCEPTION #{} - {}:\n{:#?}", T::INTERRUPT_NUMBER, T::NAME, stack_frame);

    loop {
        x86_64::instructions::hlt();
    }
}

extern "x86-interrupt" fn default_handler_with_error_code_x86<T: InterruptMetaData>(
    stack_frame: InterruptStackFrame, error_code: u64)
{
    println!("EXCEPTION #{} - {}:\n{:#?}", T::INTERRUPT_NUMBER, T::NAME, stack_frame);
}

extern "x86-interrupt" fn diverging_handler_with_error_code_x86<T: InterruptMetaData>(
    stack_frame: InterruptStackFrame, error_code: u64) -> !
{
    println!("EXCEPTION #{} - {}:\n{:#?}", T::INTERRUPT_NUMBER, T::NAME, stack_frame);

    loop {
        x86_64::instructions::hlt();
    }
}

 extern "x86-interrupt" fn page_fault_handler<T: InterruptMetaData>(
    stack_frame: InterruptStackFrame, 
    error_code: PageFaultErrorCode)
{
    println!("EXCEPTION #{} - {}:\n{:#?}", T::INTERRUPT_NUMBER, T::NAME, stack_frame);
    println!("Accessed Address: {:?}", x86_64::registers::control::Cr2::read());
    println!("Error Code: {:?}", error_code);
}

pub fn init_idt() {
    IDT.load();
}