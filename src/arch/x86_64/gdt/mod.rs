// in src/gdt.rs

use x86_64::VirtAddr;
use x86_64::structures::tss::TaskStateSegment;
use x86_64::instructions::tables::load_tss;
use x86_64::instructions::segmentation::{CS, DS, ES, FS, GS, SS, Segment};
use x86_64::structures::gdt::{GlobalDescriptorTable, Descriptor, SegmentSelector};
use lazy_static::lazy_static;

// See https://www.kernel.org/doc/Documentation/x86/kernel-stacks for a description of 
// some uses of independent stacks
// TODO: define other stacks, also a macro to create them?

pub const DOUBLE_FAULT_STACK_INDEX: u16 = 0;
pub const NMI_STACK_INDEX: u16 = 1;
pub const DEBUG_STACK_INDEX: u16 = 2;
pub const MCE_STACK_INDEX: u16 = 3;

const DEFAULT_STACK_SIZE: usize = 16384; // Arbitrary (also doesn't have to be a power of 2)
const DOUBLE_FAULT_STACK_SIZE: usize = DEFAULT_STACK_SIZE;
const NMI_STACK_SIZE: usize  = DEFAULT_STACK_SIZE;
const DEBUG_STACK_SIZE: usize = DEFAULT_STACK_SIZE;
const MCE_STACK_SIZE: usize = DEFAULT_STACK_SIZE;

static mut DOUBLE_FAULT_STACK: [u8; DOUBLE_FAULT_STACK_SIZE] = [0; DOUBLE_FAULT_STACK_SIZE];
static mut NMI_STACK: [u8; NMI_STACK_SIZE] = [0; NMI_STACK_SIZE];
static mut DEBUG_STACK: [u8; DEBUG_STACK_SIZE] = [0; DEBUG_STACK_SIZE];
static mut MCE_STACK: [u8; MCE_STACK_SIZE] = [0; MCE_STACK_SIZE];

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[DOUBLE_FAULT_STACK_INDEX as usize] = 
            VirtAddr::from_ptr(unsafe { &raw const DOUBLE_FAULT_STACK }) +
            DOUBLE_FAULT_STACK_SIZE as u64;
        tss.interrupt_stack_table[NMI_STACK_INDEX as usize] = 
            VirtAddr::from_ptr(unsafe { &raw const NMI_STACK }) +
            NMI_STACK_SIZE as u64;
        tss.interrupt_stack_table[DEBUG_STACK_INDEX as usize] = 
            VirtAddr::from_ptr(unsafe { &raw const DEBUG_STACK }) +
            DEBUG_STACK_SIZE as u64;
        tss.interrupt_stack_table[MCE_STACK_INDEX as usize] = 
            VirtAddr::from_ptr(unsafe { &raw const MCE_STACK }) +
            MCE_STACK_SIZE as u64;

        tss
    };
}


struct RingSelectors {
    code_selector: SegmentSelector,
    data_selector: SegmentSelector,
}

struct Selectors {
    ring0: RingSelectors,
    ring3: RingSelectors,
    tss_selector: SegmentSelector,
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        // TODO: do I care about order?
        let mut gdt = GlobalDescriptorTable::new();
        let mut selectors = Selectors {
            ring0: RingSelectors {
                code_selector: gdt.append(Descriptor::kernel_code_segment()),
                data_selector: gdt.append(Descriptor::kernel_data_segment()),
            },
            ring3: RingSelectors {
                code_selector: gdt.append(Descriptor::user_code_segment()),
                data_selector: gdt.append(Descriptor::user_data_segment()),
            },
            tss_selector: gdt.append(Descriptor::tss_segment(&TSS)),
        };
        (gdt, selectors)
    };
}

pub fn init() {
    // Move this into the lazy_static, and just "touch" GDT here in order 
    // to ensure it's initialized here?
    // Although other parts of the code may want to know what selectors were 
    // created
    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.ring0.code_selector);
        // Eother of these cause a GPF later on for some reason...
        // or maybe not?  Sometimes I buid and run and I get a GPF, other times it works... 
        // I don't understand...
        // No matter what, setting DS causes a GPF later on
        DS::set_reg(GDT.1.ring0.data_selector);
        ES::set_reg(GDT.1.ring0.data_selector);
        // Presumably I also need to set SS, GS and FS....
        GS::set_reg(GDT.1.ring0.data_selector);
        FS::set_reg(GDT.1.ring0.data_selector);
        SS::set_reg(GDT.1.ring0.data_selector);
        load_tss(GDT.1.tss_selector);
    }
}