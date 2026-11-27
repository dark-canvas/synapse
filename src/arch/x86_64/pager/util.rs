use x86_64::registers::control::Cr3;
use x86_64::structures::paging::page_table::PageTableEntry;
use x86_64::structures::paging::{PageTable, PageTableFlags};

use crate::pager::VirtualAddress;
use crate::types::Address;

use super::{
    PAGE_MASK_1GB, PAGE_MASK_2MB, PAGE_MASK_4KB, PAGE_OFFSET_MASK_1GB,
    PAGE_OFFSET_MASK_2MB, PAGE_OFFSET_MASK_4KB, PAGE_SIZE_1GB, PAGE_SIZE_2MB,
    PAGE_SIZE_4KB, PHYSICAL_OFFSET,
};
use super::super::X86_PAGER;

pub fn physical_mirror<T>(addr: T) -> T where T: Into<Address> + From<Address> {
    let addr = addr.into();
    let mirrored = addr + PHYSICAL_OFFSET;
    mirrored.into()
}

pub fn pages_required(size: usize) -> usize {
    (size + (PAGE_SIZE_4KB - 1)) / PAGE_SIZE_4KB
}

pub fn get_pl4_table() -> &'static mut PageTable {
    unsafe {
        let (pl4_frame, _flags) = Cr3::read();
        &mut *(physical_mirror(pl4_frame.start_address().as_u64()) as *mut PageTable)
    }
}

pub fn invalidate_page(addr: VirtualAddress) {
    unsafe {
        core::arch::asm!(
            "invlpg 0({0:r})",
            in(reg) addr.0,
            options(nostack, preserves_flags, att_syntax)
        );
    }
}

pub fn set_mmio(virtual_addr: VirtualAddress) -> bool {
    if let Some(entry) = get_entry(virtual_addr) {
    
        let new_flags = entry.flags() | (
            PageTableFlags::WRITE_THROUGH & 
            PageTableFlags::NO_CACHE);

        entry.set_flags(new_flags);

        invalidate_page(virtual_addr);
        return true;
    }
    return false;
}

pub fn get_entry(virtual_addr: VirtualAddress) -> Option<&'static mut PageTableEntry> {
    let pl4_index = (virtual_addr.0 as usize >> 39) & 0o777;
    let pl3_index = (virtual_addr.0 as usize >> 30) & 0o777;
    let pl2_index = (virtual_addr.0 as usize >> 21) & 0o777;
    let pl1_index = (virtual_addr.0 as usize >> 12) & 0o777;

    let pl4_table = get_pl4_table();

    let pl4_entry = &mut pl4_table[pl4_index];
    if pl4_entry.is_unused() {
        return None;
    }

    // page directory entry is 4kb page...
    let pl3_table = entry_to_table(&pl4_entry);
    let pl3_entry = &mut pl3_table[pl3_index];
    if pl3_entry.is_unused() {
        return None;
    }

    if pl3_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
        return Some(pl3_entry);
    }

    // this could be a 2mb page...
    let pl2_table = entry_to_table(&pl3_entry);
    let pl2_entry = &mut pl2_table[pl2_index];
    if pl2_entry.is_unused() {
        return None;
    }

    if pl2_entry.flags().contains(x86_64::structures::paging::PageTableFlags::HUGE_PAGE) {
        return Some(pl2_entry);
    }

    let pl1_table = entry_to_table(&pl2_entry);
    let pl1_entry = &mut pl1_table[pl1_index];
    if pl1_entry.is_unused() {
        return None;
    }

    Some(pl1_entry)
}


pub(super) fn entry_to_table(entry: &PageTableEntry) -> &'static mut PageTable {
    unsafe {
        &mut *(physical_mirror(entry.addr().as_u64()) as *mut PageTable)
    }
}

pub fn is_1gb_aligned(addr: Address) -> bool {
    addr & PAGE_OFFSET_MASK_1GB == 0
}

pub fn is_2mb_aligned(addr: Address) -> bool {
    addr & PAGE_OFFSET_MASK_2MB == 0
}

pub fn is_4kb_aligned(addr: Address) -> bool {
    addr & PAGE_OFFSET_MASK_4KB == 0
}

pub fn next_1gb_page(addr: Address) -> Address {
    (addr + PAGE_SIZE_1GB as Address) & PAGE_MASK_1GB
}

pub fn next_2mb_page(addr: Address) -> Address {
    (addr + PAGE_SIZE_2MB as Address) & PAGE_MASK_2MB
}

pub fn next_4kb_page(addr: Address) -> Address {
    (addr + PAGE_SIZE_4KB as Address) & PAGE_MASK_4KB
}

#[allow(dead_code)]
fn breakpoint() {
    println!("Artificial breakpoint");
    loop {
        x86_64::instructions::hlt();
    }
}

pub fn get_kernel_cr3() -> u64 {
    X86_PAGER.get().unwrap().get_kernel_cr3()
}
