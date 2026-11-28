use crate::types::Address;

pub const PAGER_MAX_SUPPORTED_MEMORY: usize = 512*1024*1024*1024; // 512GB

pub const PAGE_SIZE_4KB: usize = 4*1024;
pub const PAGE_SIZE_2MB: usize = 2*1024*1024;
pub const PAGE_SIZE_1GB: usize = 1*1024*1024*1024;

pub const PAGE_OFFSET_MASK_4KB: Address = 4*1024-1;
pub const PAGE_OFFSET_MASK_2MB: Address = 2*1024*1024-1;
pub const PAGE_OFFSET_MASK_1GB: Address = 1*1024*1024*1024-1;

pub const PAGE_MASK_4KB: Address = !PAGE_OFFSET_MASK_4KB;
pub const PAGE_MASK_2MB: Address = !PAGE_OFFSET_MASK_2MB;
pub const PAGE_MASK_1GB: Address = !PAGE_OFFSET_MASK_1GB;

pub(crate) const PHYSICAL_OFFSET: Address = 0xFFFFFF0000000000;

pub(super) const PAGE_STACK_4KB_BASE: Address = 0xFFFFFFD000000000;
pub(super) const PAGE_STACK_2MB_BASE: Address = 0xFFFFFFE000000000;
pub(super) const PAGE_STACK_1GB_BASE: Address = 0xFFFFFFE000201000;

pub(super) const PAGE_STACK_4KB_MAX_PAGES: usize = 134217728;
pub(super) const PAGE_STACK_2MB_MAX_PAGES: usize = 262144;
pub(super) const PAGE_STACK_1GB_MAX_PAGES: usize = 512;

pub(super) const PAGE_AGGREGATOR_2MB_BASE: Address = 0xFFFFFFD040000000;
pub(super) const PAGE_AGGREGATOR_1GB_BASE: Address = 0xFFFFFFE000200000;
pub(super) const PAGE_AGGREGATOR_512GB_BASE: Address = 0xFFFFFFE000202000;
