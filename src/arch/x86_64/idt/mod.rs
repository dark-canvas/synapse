pub mod handlers;

use spin::Lazy;
use x86_64::structures::idt::InterruptDescriptorTable;

use super::gdt::ExceptionStackIndex;
use super::pit::timer_interrupt_handler;

use self::handlers::default_handler_x86;
use self::handlers::diverging_handler_x86;
use self::handlers::default_handler_with_error_code_x86;
use self::handlers::diverging_handler_with_error_code_x86;
use self::handlers::page_fault_handler;

use self::handlers::SecurityExceptionMetaData;
use self::handlers::VMMCommunicationExceptionMetaData;
use self::handlers::HVInjectionExceptionMetaData;
use self::handlers::CpProtectionExceptionMetaData;
use self::handlers::VirtualizationMetaData;
use self::handlers::SimdFloatingPointMetaData;
use self::handlers::MachineCheckMetaData;
use self::handlers::AlignmentCheckMetaData;
use self::handlers::X87FloatingPointMetaData;
use self::handlers::PageFaultMetaData;
use self::handlers::GeneralProtectionFaultMetaData;
use self::handlers::StackSegmentFaultMetaData;
use self::handlers::SegmentNotPresentMetaData;
use self::handlers::InvalidTssMetaData;
use self::handlers::DoubleFaultMetaData;
use self::handlers::DeviceNotAvailableMetaData;
use self::handlers::InvalidOpcodeMetaData;
use self::handlers::BoundRangeExceededMetaData;
use self::handlers::OverflowMetaData;
use self::handlers::BreakpointMetaData;
use self::handlers::NonMaskableInterruptMetaData;
use self::handlers::DebugMetaData;
use self::handlers::DivideByZeroMetaData;

// TODO: use this in the interrupt meta data structs?
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    DivideByZero = 0,
    Debug = 1,
    NonMaskableInterrupt = 2,
    Breakpoint = 3,
    Overflow = 4,
    BoundRangeExceeded = 5,
    InvalidOpcode = 6,
    DeviceNotAvailable = 7,
    DoubleFault = 8,
    InvalidTss = 10,
    SegmentNotPresent = 11,
    StackSegmentFault = 12,
    GeneralProtectionFault = 13,
    PageFault = 14,
    X87FloatingPoint = 16,
    AlignmentCheck = 17,
    MachineCheck = 18,
    SimdFloatingPoint = 19,
    Virtualization = 20,
    CpProtectionException = 21,
    HVInjectionException = 22,
    VMMCommunicationException = 23,
    SecurityException = 30,
    Timer = 32,
    Error = 33,
    Spurious = 34,
}

static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
        let mut idt = InterruptDescriptorTable::new();
        idt.divide_error.set_handler_fn(default_handler_x86::<DivideByZeroMetaData>);
        unsafe {
            idt.debug.set_handler_fn(default_handler_x86::<DebugMetaData>)
                     .set_stack_index(ExceptionStackIndex::DebugStackIndex as u16);
            idt.non_maskable_interrupt.set_handler_fn(default_handler_x86::<NonMaskableInterruptMetaData>)
                                    .set_stack_index(ExceptionStackIndex::NmiStackIndex as u16);
        }

        idt.breakpoint.set_handler_fn(default_handler_x86::<BreakpointMetaData>);
        idt.overflow.set_handler_fn(default_handler_x86::<OverflowMetaData>);
        idt.bound_range_exceeded.set_handler_fn(default_handler_x86::<BoundRangeExceededMetaData>);
        idt.invalid_opcode.set_handler_fn(default_handler_x86::<InvalidOpcodeMetaData>);
        idt.device_not_available.set_handler_fn(default_handler_x86::<DeviceNotAvailableMetaData>);
        unsafe {
            idt.double_fault.set_handler_fn(diverging_handler_with_error_code_x86::<DoubleFaultMetaData>)
                            .set_stack_index(ExceptionStackIndex::DoubleFaultStackIndex as u16);
        }
        idt.invalid_tss.set_handler_fn(default_handler_with_error_code_x86::<InvalidTssMetaData>);
        idt.segment_not_present.set_handler_fn(default_handler_with_error_code_x86::<SegmentNotPresentMetaData>);
        idt.stack_segment_fault.set_handler_fn(default_handler_with_error_code_x86::<StackSegmentFaultMetaData>);
        idt.general_protection_fault.set_handler_fn(default_handler_with_error_code_x86::<GeneralProtectionFaultMetaData>);
        idt.page_fault.set_handler_fn(page_fault_handler::<PageFaultMetaData>);
        idt.x87_floating_point.set_handler_fn(default_handler_x86::<X87FloatingPointMetaData>);
        idt.alignment_check.set_handler_fn(default_handler_with_error_code_x86::<AlignmentCheckMetaData>);
        unsafe {
            idt.machine_check.set_handler_fn(diverging_handler_x86::<MachineCheckMetaData>)
                             .set_stack_index(ExceptionStackIndex::MceStackIndex as u16);
        }
        idt.simd_floating_point.set_handler_fn(default_handler_x86::<SimdFloatingPointMetaData>);
        idt.virtualization.set_handler_fn(default_handler_x86::<VirtualizationMetaData>);
        idt.cp_protection_exception.set_handler_fn(default_handler_with_error_code_x86::<CpProtectionExceptionMetaData>);
        idt.hv_injection_exception.set_handler_fn(default_handler_x86::<HVInjectionExceptionMetaData>);
        idt.vmm_communication_exception.set_handler_fn(default_handler_with_error_code_x86::<VMMCommunicationExceptionMetaData>);
        idt.security_exception.set_handler_fn(default_handler_with_error_code_x86::<SecurityExceptionMetaData>);
        idt[InterruptIndex::Timer as u8].set_handler_fn(timer_interrupt_handler);
        idt
});

pub fn init_idt() {
    IDT.load();

    // quick test...
    //x86_64::instructions::interrupts::int3(); 
}