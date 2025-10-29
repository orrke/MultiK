/*
    This file is for the initialization of the OS on the BSP (which is the only CPU to run initially)
    It has a slightly different loop at first (starting up the APs, allocating their stacks and capability DBs)
    But it then comes back into a regular kernel loop
*/

use core::ptr::null_mut;
use x86_64::registers::model_specific::Msr;
use crate::kernel::{hcf, HHDM_REQUEST, MP_REQUEST};
use crate::kernel::cpu_drv::bootstrapping::ap::AP_ENTRY;
use crate::kernel::cpu_drv::interrupts::interrupts_setup;
use crate::kernel::mm::init_bootstrap_alloc;
use crate::kernel::MAX_CPUS;

static mut PER_CPU_DATA: [PerCPUData; MAX_CPUS] = [PerCPUData::new(); MAX_CPUS];

const LAPIC_BASE: *mut u32 = 0xFEE00000 as *mut u32;

//const LAPIC_ICR_HIGH: usize = 0x310;
const LAPIC_ICR_LOW: usize = 0x300;

#[allow(unsafe_op_in_unsafe_fn, static_mut_refs)]
pub unsafe fn bsp_main() -> ! {
    let mut alloc = init_bootstrap_alloc();
    let mp_response = MP_REQUEST.get_response().unwrap();

    //initialize AP entry point
    AP_ENTRY.init();

    let cpus = mp_response.cpus();

    for cpu in cpus {

        let lapic = mp_response.bsp_lapic_id();
        let target_apic = cpu.lapic_id;
        let capability_db_ptr = alloc.alloc(4096);

        if lapic != target_apic {

            let target_apic = cpu.lapic_id;

            reset_and_wake_up_ap(target_apic);


        }

        //now we only need to tell the CPUs the capability DB pointers.
        //Also, we consider this one like the others, because it also needs a capability DB in the beginning.
        PER_CPU_DATA[target_apic as usize].cap_db_ptr = capability_db_ptr;
    }

    //setup interrupts for this core
    interrupts_setup();

    super::super::kernel_main();
}

#[allow(unsafe_op_in_unsafe_fn, unused, static_mut_refs)]
unsafe fn reset_and_wake_up_ap(target_apic: u32) {
    let hhdm_offset = HHDM_REQUEST.get_response().unwrap().offset() as usize;

    let mut icr_high = *((hhdm_offset + 0x310 + LAPIC_BASE as usize) as *mut u32);
    let mut icr_low = *((hhdm_offset + 0x300 + LAPIC_BASE as usize) as *mut u32);

    //wait for the ICR to be idle before sending the INIT IPI
    wait_icr_idle();

    icr_high = target_apic << 24;
    icr_low = (0b101 << 8) | (1 << 14);

    let ap_start = &AP_ENTRY as *const _ as u64;

    //wait for the ICR to be idle before sending the SIPI
    wait_icr_idle();

    let sipi_vector = ap_start >> 12;

    unsafe {
        LAPIC_BASE.add(LAPIC_ICR_LOW).add(hhdm_offset).write_volatile(
            (0b110 << 8) | (sipi_vector as u32) // Delivery mode: Startup IPI, vector: sipi_vector
        );
    }
}

fn wait_icr_idle() {
    let hhdm_offset = HHDM_REQUEST.get_response().unwrap().offset() as usize;

    while unsafe { LAPIC_BASE.add(LAPIC_ICR_LOW).add(hhdm_offset).read_volatile() } & (1 << 12) != 0 {}
}

#[derive(Copy, Clone, Debug)]
struct PerCPUData {
    cap_db_ptr: *mut u8,
    //potentially other things.
}

impl PerCPUData {
    pub const fn new() -> Self {
        PerCPUData {
            cap_db_ptr: null_mut(),
        }
    }
}
