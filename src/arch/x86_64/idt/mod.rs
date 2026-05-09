pub mod handlers;

use lazy_static::lazy_static;
use x86_64::structures::idt::InterruptDescriptorTable;

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

pub fn init_idt() {
    IDT.load();
}