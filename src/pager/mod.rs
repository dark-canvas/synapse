use atomic_refcell::AtomicRefCell;
use thiserror::Error;
use crate::errors::ErrCode;
use crate::arch::x86_64::pager::{PhysicalAddress, VirtualAddress};

pub mod on_demand_array;
pub mod on_demand_stack;

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum PagerError {
    #[error("physical address not found")]
    PhysicalAddressNotFound(PhysicalAddress),
    #[error("virtual address not found")]
    VirtualAddressNotFound(VirtualAddress),
    #[error("unmapped virtual address")]
    UnmappedVirtualAddress(VirtualAddress),
    #[error("virtual address already mapped")]
    VirtualAddressAlreadyMapped(VirtualAddress),
}

///static Lazy<&dyn Pager> = Lazy::new(|| { /* pager... */ }

// Defines a generic cross-platform pager implementation usble by certain primatives like page_based_list
// The pager is global, and so must make use of interior mutability.
// Consider:
// - Returns physical, or virtual, or both?
// - Exposes mapping routines?
#[allow(dead_code)]
pub trait Pager: Sync {
    fn get_page_size(&self) -> usize;
    fn get_page_size_log2(&self) -> usize {
        /// Implementations will likely want to override this, as the result is a static number
        let page_size = self.get_page_size();
        let mut log2 = 0;
        let mut size = page_size;
        while size > 1 {
            size >>= 1;
            log2 += 1;
        }
        log2
    }

    fn allocate_physical(&self) -> Result<PhysicalAddress, ErrCode>;
    // Or just use the UEFI crate?
    //fn allocate_physical_if<F>(&self, page_cond: F) -> Result<PhysicalAddress, ErrCode>
    //  where F: Fn(PhysicalAddress) -> bool;
    fn free_physical(&self, addr: PhysicalAddress)-> Result<(), ErrCode>;

    fn allocate_virtual(&self, num: usize, to_addr: VirtualAddress) -> Result<VirtualAddress, ErrCode>;
    fn free_virtual(&self, num: usize, base_addr: VirtualAddress) -> Result<(), ErrCode>;

    fn map_physical_to_virtual(&self, phys: PhysicalAddress, virt: VirtualAddress) -> Result<(), ErrCode>;

    // TODO: at least get physical should reutrn an Option... what about get_virtual?
    // For portability, they should both return Option
    fn get_virtual_address(&self, addr: PhysicalAddress) -> Result<VirtualAddress, ErrCode>;
    fn get_physical_address(&self, addr: VirtualAddress) -> Result<PhysicalAddress, ErrCode>;

    fn ensure_mapped(&self, virtual_addr: VirtualAddress) -> Result<(), ErrCode> {
        match get_pager().allocate_virtual(1, virtual_addr) {
            Ok(_) => Ok(()),
            Err(ErrCode::Pager(PagerError::VirtualAddressAlreadyMapped(_))) => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn ensure_mapped_range(&self, virtual_addr: VirtualAddress, num_pages: usize) -> Result<(), ErrCode> {
        let page_size = get_pager().get_page_size();
        for i in 0..num_pages {
            let addr = virtual_addr + (i * page_size);
            self.ensure_mapped(addr)?;
        }
        Ok(())
    }
}

struct NullPager{}

impl Pager for NullPager {
    fn get_page_size(&self) -> usize { 0 }
    fn allocate_physical(&self) -> Result<PhysicalAddress, ErrCode> { Err(ErrCode::Unimplemented) }
    fn free_physical(&self, _addr: PhysicalAddress) -> Result<(), ErrCode> { Err(ErrCode::Unimplemented) }
    fn allocate_virtual(&self, _num: usize, _to_addr: VirtualAddress) -> Result<VirtualAddress, ErrCode> { Err(ErrCode::Unimplemented) }
    fn free_virtual(&self, _num: usize, _base_addr: VirtualAddress) -> Result<(), ErrCode> { Err(ErrCode::Unimplemented) }
    fn map_physical_to_virtual(&self, _phys: PhysicalAddress, _virt: VirtualAddress) -> Result<(), ErrCode> { Err(ErrCode::Unimplemented) }
    fn get_virtual_address(&self, _addr: PhysicalAddress) -> Result<VirtualAddress, ErrCode> { Err(ErrCode::Unimplemented) }
    fn get_physical_address(&self, _addr: VirtualAddress) -> Result<PhysicalAddress, ErrCode> { Err(ErrCode::Unimplemented) }
}

// The PAGER static global is expected to be replaced with the real, arch-specific, pager at startup
static NULL_PAGER: NullPager = NullPager{};
pub static PAGER: AtomicRefCell<&dyn Pager> = AtomicRefCell::new( &NULL_PAGER );

pub fn get_pager() -> &'static dyn Pager {
    *PAGER.borrow()
}