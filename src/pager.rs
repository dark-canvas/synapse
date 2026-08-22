use atomic_refcell::AtomicRefCell;
use thiserror::Error;
use crate::errors::ErrCode;
use crate::arch::x86_64::pager::{PhysicalAddress, VirtualAddress};

#[derive(Error, Debug)]
pub enum PagerError {
    #[error("physical address not found")]
    PhysicalAddressNotFound(PhysicalAddress),
    #[error("virtual address not found")]
    VirtualAddressNotFound(VirtualAddress),
    #[error("unmapped virtual address")]
    UnmappedVirtualAddress(VirtualAddress),
}

///static Lazy<&dyn Pager> = Lazy::new(|| { /* pager... */ }

// Defines a generic cross-platform pager implementation usble by certain primatives like page_based_list
// The pager is global, and so must make use of interior mutability.
// Consider:
// - Returns physical, or virtual, or both?
// - Exposes mapping routines?
pub trait Pager: Sync {
    fn get_page_size(&self) -> usize;
    
    // TODO: possibly these should return a Result<Addr, &str> instead?
    // Or should we start using somethig like?
    //    Result<Addr, ErrCode>
    //    Result<Addr, PagerErrorCode>

    // TODO: Address -> Physical/VirtualAddress
    fn allocate_physical(&self) -> Result<PhysicalAddress, ErrCode>;
    // Or just use the UEFI crate?
    //fn allocate_physical_if<F>(&self, page_cond: F) -> Result<PhysicalAddress, ErrCode>
    //  where F: Fn(PhysicalAddress) -> bool;
    fn free_physical(&self, addr: PhysicalAddress)-> Result<(), ErrCode>;

    // WHich?
    // First one assumes the code can find a free block, which doesn't 
    // seem like it's the pager's responsibility, so probably the secnod one?
    //allocate_virtual(num: usize) -> VirtualAddress;
    fn allocate_virtual(&self, num: usize, to_addr: VirtualAddress) -> Result<VirtualAddress, ErrCode>;
    fn free_virtual(&self, num: usize, base_addr: VirtualAddress) -> Result<(), ErrCode>;

    fn map_physical_to_virtual(&self, phys: PhysicalAddress, virt: VirtualAddress) -> Result<(), ErrCode>;

    // TODO: at least get physical should reutrn an Option... what about get_virtual?
    // For portability, they should both return Option
    fn get_virtual_address(&self, addr: PhysicalAddress) -> Result<VirtualAddress, ErrCode>;
    fn get_physical_address(&self, addr: VirtualAddress) -> Result<PhysicalAddress, ErrCode>;
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