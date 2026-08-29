// in src/gdt.rs

use x86_64::VirtAddr;
use x86_64::structures::tss::TaskStateSegment;
use x86_64::instructions::tables::load_tss;
use x86_64::instructions::segmentation::{CS, DS, ES, FS, GS, SS, Segment};
use x86_64::structures::gdt::{
    GlobalDescriptorTable, Descriptor, SegmentSelector};
use x86_64::structures::DescriptorTablePointer;
use x86_64::instructions::tables::lgdt;
use spin::Lazy;
use crate::types::CpuId;
use crate::arch::x86_64::smp::per_cpu_data;
use crate::arch::x86_64::smp::per_cpu_data::get_exception_stack_base;

// See https://www.kernel.org/doc/Documentation/x86/kernel-stacks for a description of 
// some uses of independent stacks
// TODO: define other stacks, also a macro to create them?

#[derive(Clone, Copy)]
pub enum ExceptionStackIndex {
    DoubleFaultStackIndex,
    NmiStackIndex,
    DebugStackIndex,
    MceStackIndex,
}

pub const RING_0_CODE_SELECTOR : u16 = 0x08;
pub const RING_0_DATA_SELECTOR : u16 = 0x10;
pub const RING_3_CODE_SELECTOR : u16 = 0x18;
pub const RING_3_DATA_SELECTOR : u16 = 0x20;
pub const TSS_SELECTOR : u16 = 0x28;

pub const GDT_SIZE: usize = 56; // null + (kernel + user)(code + data) + tss descriptors = 5 * 8 + 16 

pub unsafe fn create_tss(cpu_id: CpuId) {
    let tss_addr = per_cpu_data::get_tss_base(cpu_id);
    let tss_ptr = tss_addr.0 as *mut TaskStateSegment;
    
    // setup up initial default values (effectively construct the TSS in place)
    tss_ptr.write(TaskStateSegment::new());
    
    for stack in [ 
        ExceptionStackIndex::DoubleFaultStackIndex,
        ExceptionStackIndex::NmiStackIndex,
        ExceptionStackIndex::DebugStackIndex,
        ExceptionStackIndex::MceStackIndex ] {

        (*tss_ptr).interrupt_stack_table[stack as usize] = 
            VirtAddr::new(
                get_exception_stack_base(cpu_id, stack).0
            );
    }
}

pub unsafe fn create_gdt(cpu_id: CpuId) {
    let tss_addr = per_cpu_data::get_tss_base(cpu_id);
    let tss_ptr = tss_addr.0 as *const TaskStateSegment;

    let gdt_addr = per_cpu_data::get_gdt_base(cpu_id);
    let mut gdt_ptr = gdt_addr.0 as *mut u64;
    
    for descriptor in [ 
        Descriptor::UserSegment(0), // null descriptor
        Descriptor::kernel_code_segment(),
        Descriptor::kernel_data_segment(),
        Descriptor::user_code_segment(),
        Descriptor::user_data_segment(),
        Descriptor::tss_segment_unchecked(tss_ptr),
    ] {
        match(descriptor) {
            Descriptor::UserSegment(raw) => {
                gdt_ptr.write(raw);
                gdt_ptr = gdt_ptr.add(1);
            },
            Descriptor::SystemSegment(lower, upper) => {
                gdt_ptr.write(lower);
                gdt_ptr = gdt_ptr.add(1);
                gdt_ptr.write(upper);
                gdt_ptr = gdt_ptr.add(1);
            }
        }
    }
}

pub unsafe fn load_gdt(cpu_id: CpuId) {
    let gdt_pointer = DescriptorTablePointer {
        limit: (GDT_SIZE - 1) as u16, 
        base: VirtAddr::new(per_cpu_data::get_gdt_base(cpu_id).0),
    };
    lgdt(&gdt_pointer);
}

pub fn init(cpu_id: CpuId) {
    unsafe {
        create_tss(cpu_id);
        create_gdt(cpu_id);
        load_gdt(cpu_id);

        CS::set_reg(SegmentSelector(RING_0_CODE_SELECTOR));

        DS::set_reg(SegmentSelector(RING_0_DATA_SELECTOR));
        ES::set_reg(SegmentSelector(RING_0_DATA_SELECTOR));
        SS::set_reg(SegmentSelector(RING_0_DATA_SELECTOR));
 
        load_tss(SegmentSelector(TSS_SELECTOR));
    }
}