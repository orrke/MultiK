mod capabilities;
mod interrupts;
pub mod bootstrapping;

use crate::kernel::hcf;

pub unsafe fn kernel_main() -> ! {
    hcf();
}