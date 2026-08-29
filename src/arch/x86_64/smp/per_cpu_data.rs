use crate::types::CpuId;
use super::VirtualAddress;
use crate::arch::x86_64::gdt::ExceptionStackIndex;
use crate::arch::x86_64::gdt::GDT_SIZE;
use crate::pager::PAGER;

/// Per-CPU Data
///
/// Each CPU (the BSP and each AP) has a well known region of memory allocated to it which 
/// contains it's stack, as well as a region of state unique to it (data which the CPU can 
/// use to determine which CPU number it is, and where it's scheduler structures are).
///
/// Each CPU core is provided 2MB of space.
///
/// The extents of this reagion can be calculated based on the CpuID of the executing core.
/// Because the core doesn't know it's CpuID when it initially boots, the gs register is 
/// pre-configured (via the trampoline) to point to the CpuState structure for that 
/// specific core (and the CpuState includes the CpuID).
///
/// The region of memory looks like (higher addresses on top):
///
///  PerCpuData
/// |-----------|   <----- Stack Base, expanding down
/// |           |                   |
/// | Cpu Stack |                   |
/// |           |                   V
/// |           |
/// |-----------|   <----- Top of stack
///
///    Unmapped     <----- Allows for page fault on stack overflow
///
/// |-----------|
/// |  CpuState |
/// |-----------|

// This becomes awkward to visualize... rather than a top, and subtracting from it... probably we want a base and add to it...
// from 0xFFFFFFF000000000 to the top of the virtual address range there is 64GB of space, which is enough room for 32k CPU state regions
pub const ALLOCATION_BASE: VirtualAddress = VirtualAddress(0xFFFFFFF000000000);

// 2MB per-cpu data
// 1MB stack and then 1mb of per-cpu state?
// 1MB stack and intentionally unmapped page and then per-cpu state?
pub const ALLOCATION_SIZE: usize = 1*1024*1024;
pub const STACK_SIZE: usize = ALLOCATION_SIZE - 20 * 4096;

const EXCEPTION_STACK_SIZE: usize = 16384; 

/*
pub fn get_top(id: CpuId) -> VirtualAddress {
    CPU_CORE_STACK_SPACE_TOP - (id as usize * PER_CPU_ALLOCATION_SIZE)
}
*/

pub fn get_base(id: CpuId) -> VirtualAddress {
    ALLOCATION_BASE + (id as usize * ALLOCATION_SIZE)
}

// stack starts at the top of the per-cpu alloction, and 
// expands down
pub fn get_stack_base(id: CpuId) -> VirtualAddress {
    get_base(id) + ALLOCATION_SIZE
}


pub fn get_exception_stack_base(id: CpuId, which: ExceptionStackIndex) -> VirtualAddress {
    get_base(id) + 0x2000 + (which as usize * EXCEPTION_STACK_SIZE)
}

pub fn get_tss_base(id: CpuId) -> VirtualAddress {
    get_gdt_base(id) + GDT_SIZE
}

pub fn get_gdt_base(id: CpuId) -> VirtualAddress {
    get_base(id) + 0x1000
}

pub fn get_stack_top(id: CpuId) -> VirtualAddress {
    get_stack_base(id) - STACK_SIZE
}

// between the stack and the CpuState struct, there's an section 
// of unmapped pages (which will therefore result in a page fault if 
// the kernel stack overflows)

// cpu state is at the bottom of the per-cpu allocation
pub fn get_cpu_state_base(id: CpuId) -> VirtualAddress {
    get_base(id)
}

// TODO: a version of this is duplicated in the bootloader for the BSP (create_kernel_cpu_state)
// Would be nice to share it somehow
pub fn create_all(id: CpuId) {
    let pager = PAGER.borrow();

    let page_size = pager.get_page_size();
    let num_pages = ALLOCATION_SIZE / page_size;
    let per_cpu_base = get_base(id);
    pager.allocate_virtual(num_pages, per_cpu_base).unwrap();

    let non_stack_pages = 2;
    let stack_pages = num_pages - non_stack_pages;
    let stack_base = per_cpu_base + non_stack_pages * page_size;

    // zero out the CpuState and GDT, and write sentinel values to all the stacks
    unsafe { core::ptr::write_bytes(per_cpu_base.0 as *mut u8, 0x0, non_stack_pages*page_size); }
    unsafe { core::ptr::write_bytes(stack_base.0 as *mut u8, 0xa5, stack_pages*page_size); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_bsp_pointers() {
        let bsp : CpuId = 0;
        assert_eq!(get_stack_base(bsp), VirtualAddress(0xFFFFFFF000100000));
        //assert_eq!(get_stack_top(bsp), VirtualAddress(0xFFFFFFF000100000));
        assert_eq!(get_exception_stack_base(bsp, ExceptionStackIndex::DoubleFaultStackIndex), VirtualAddress(0xFFFFFFF000002000));
        assert_eq!(get_exception_stack_base(bsp, ExceptionStackIndex::NmiStackIndex), VirtualAddress(0xFFFFFFF000006000));
        assert_eq!(get_exception_stack_base(bsp, ExceptionStackIndex::DebugStackIndex), VirtualAddress(0xFFFFFFF00000a000));
        assert_eq!(get_exception_stack_base(bsp, ExceptionStackIndex::MceStackIndex), VirtualAddress(0xFFFFFFF00000e000));
        assert_eq!(get_tss_base(bsp), VirtualAddress(0xFFFFFFF000001038));
        assert_eq!(get_gdt_base(bsp), VirtualAddress(0xFFFFFFF000001000));
        assert_eq!(get_cpu_state_base(bsp), VirtualAddress(0xFFFFFFF000000000));
    }

    #[test]
    fn test_get_cpu_n_pointers() {
        let cpu : CpuId = 3;
        assert_eq!(get_stack_base(cpu), VirtualAddress(0xFFFFFFF000400000));
        assert_eq!(get_exception_stack_base(cpu, ExceptionStackIndex::DoubleFaultStackIndex), VirtualAddress(0xFFFFFFF000302000));
        assert_eq!(get_exception_stack_base(cpu, ExceptionStackIndex::NmiStackIndex), VirtualAddress(0xFFFFFFF000306000));
        assert_eq!(get_exception_stack_base(cpu, ExceptionStackIndex::DebugStackIndex), VirtualAddress(0xFFFFFFF00030a000));
        assert_eq!(get_exception_stack_base(cpu, ExceptionStackIndex::MceStackIndex), VirtualAddress(0xFFFFFFF00030e000));
        assert_eq!(get_tss_base(cpu), VirtualAddress(0xFFFFFFF000301038));
        assert_eq!(get_gdt_base(cpu), VirtualAddress(0xFFFFFFF000301000));
        assert_eq!(get_cpu_state_base(cpu), VirtualAddress(0xFFFFFFF000300000));
    }
}