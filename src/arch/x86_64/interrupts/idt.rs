use spin::Lazy;
use x86_64::structures::idt::InterruptDescriptorTable;

use crate::arch::x86_64::pager::invalidate_page_handler;

use super::super::gdt::ExceptionStackIndex;
use super::super::scheduler::timer_interrupt_handler;
use super::handlers::{
    default_handler_x86, diverging_handler_x86, default_handler_with_error_code_x86,
    diverging_handler_with_error_code_x86, page_fault_handler, AlignmentCheckMetaData,
    BreakpointMetaData, BoundRangeExceededMetaData, CpProtectionExceptionMetaData,
    DebugMetaData, DeviceNotAvailableMetaData, DivideByZeroMetaData, DoubleFaultMetaData,
    GeneralProtectionFaultMetaData, HVInjectionExceptionMetaData, InvalidOpcodeMetaData,
    InvalidTssMetaData, MachineCheckMetaData, NonMaskableInterruptMetaData, OverflowMetaData,
    PageFaultMetaData, SecurityExceptionMetaData, SegmentNotPresentMetaData,
    SimdFloatingPointMetaData, StackSegmentFaultMetaData, VirtualizationMetaData,
    VMMCommunicationExceptionMetaData, X87FloatingPointMetaData,
};
use super::InterruptIndex;

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
    idt[InterruptIndex::InvalidatePage as u8].set_handler_fn(invalidate_page_handler);
    idt
});

pub fn init_idt() {
    IDT.load();

    // quick test...
    //x86_64::instructions::interrupts::int3();
}
