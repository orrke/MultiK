use crate::kernel::mm::bootstrap::BumpAlloc;

mod bootstrap;
mod pmem;
mod vmem;
mod table;

//2 symbols for the bootstrap region we'll be using for this allocator
unsafe extern "C" {
    static mut _bootstrap_start: u64;
    static mut _bootstrap_end: u64;
}

#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn init_bootstrap_alloc() -> BumpAlloc {
    let mut alloc = BumpAlloc::new();
    
    alloc.init(_bootstrap_start as *mut u8, _bootstrap_end as *mut u8);
    
    alloc
}