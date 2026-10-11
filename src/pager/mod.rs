#[cfg(test)]
pub mod test_helpers;
mod on_demand_array;
mod on_demand_stack;
mod physical_address;
mod virtual_address;
mod pager;

pub use self::physical_address::PhysicalAddress;
pub use self::virtual_address::VirtualAddress;
pub use self::pager::Pager;
pub use self::pager::PagerError;
pub use self::pager::PAGER; // TODO: remove this (inject pager to all consumers)
pub use self::pager::get_pager;
pub use self::on_demand_array::OnDemandArray;
pub use self::on_demand_stack::OnDemandStack;

#[cfg(test)]
pub use self::pager::MockPager;