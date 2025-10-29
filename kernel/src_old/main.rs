#![no_std]
#![no_main]

#![feature(abi_x86_interrupt)]
extern crate alloc;


#[allow(arithmetic_overflow)]

pub mod memory;
pub mod interrupts;
pub mod task;
pub mod shell;

use core::arch::asm;
use spin::Mutex;

use limine::request::{ExecutableAddressRequest, FramebufferRequest, HhdmRequest, MemoryMapRequest};
use limine::BaseRevision;
use limine::framebuffer::Framebuffer;
use crate::interrupts::interrupts_setup;
use crate::memory::init_kernel_allocators;
use crate::shell::text::init_framebuffer;
use crate::task::executor::Executor;
use crate::task::Task;

//limine requests
#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();
#[used]
#[unsafe(link_section = ".requests")]
pub static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
pub static MEMMAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

#[used]
#[unsafe(link_section = ".kernel_start")]
pub static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[unsafe(link_section = ".kernel_end")]
pub static EXEC_REQUEST: ExecutableAddressRequest = ExecutableAddressRequest::new();

//end of limine requests

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    interrupts_setup();

    assert!(BASE_REVISION.is_supported());

    init_framebuffer();

    println!("Initialized framebuffer");

    println!("About to initialize the heap");

    init_kernel_allocators();

    println!("Initialized heap");

    println!("Hello, World!");

    let mut executor = Executor::new();
    executor.spawn(Task::new(task::keyboard::print_keypresses()));
    executor.run();

    //hcf();
}

#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    println!("\n{}", info);

    hcf();
}

fn hcf() -> ! {
    loop {
        unsafe {
            asm!("hlt")
        }
    }
}