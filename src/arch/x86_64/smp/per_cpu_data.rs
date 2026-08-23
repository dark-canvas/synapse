use crate::types::CpuId;
use super::VirtualAddress;

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
pub const ALLOCATION_SIZE: usize = 2*1024*1024;
pub const STACK_SIZE: usize = 1*1024*1024;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_bsp_pointers() {
        let bsp : CpuId = 0;
        assert_eq!(get_stack_base(bsp), VirtualAddress(0xFFFFFFF000200000));
        assert_eq!(get_stack_top(bsp), VirtualAddress(0xFFFFFFF000100000));
        assert_eq!(get_cpu_state_base(bsp), VirtualAddress(0xFFFFFFF000000000));
    }

    #[test]
    fn test_get_cpu_n_pointers() {
        let bsp : CpuId = 3;
        assert_eq!(get_stack_base(bsp), VirtualAddress(0xFFFFFFF000800000));
        assert_eq!(get_stack_top(bsp), VirtualAddress(0xFFFFFFF000700000));
        assert_eq!(get_cpu_state_base(bsp), VirtualAddress(0xFFFFFFF000600000));
    }
}