use core::arch::asm;
use limine::BaseRevision;
use limine::request::{ExecutableAddressRequest, HhdmRequest, MpRequest};
use crate::kernel::cpu_drv::bootstrapping::bsp::bsp_main;
use crate::kernel::cpu_drv::bootstrapping::disable_ap;

mod api;
mod cpu_drv;
mod mm;

#[allow(arithmetic_overflow)]

//limine requests
#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests")]
static MP_REQUEST: MpRequest = MpRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static EXEC_REQUEST: ExecutableAddressRequest = ExecutableAddressRequest::new();
//end of limine requests

pub const MAX_CPUS: usize = 512;

//this is the initial entry point.
#[allow(unsafe_op_in_unsafe_fn)]
#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    disable_ap();

    //this function should logically only launch on the BSP, thus launching a single kernel only.
    bsp_main();
}

#[panic_handler]
fn rust_panic(_: &core::panic::PanicInfo) -> ! {
    hcf();
}

fn hcf() -> ! {
    loop {
        unsafe {
            asm!("hlt")
        }
    }
}


