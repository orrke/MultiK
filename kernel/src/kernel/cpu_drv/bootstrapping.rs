use x86_64::registers::model_specific::Msr;
use crate::kernel::{hcf, HHDM_REQUEST, MP_REQUEST};

pub mod ap;
pub mod bsp;

//I'm not gonna lie on this one, chatgpt helped, but come on man, what the heck is that.
#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn disable_ap() {
    const IA32_APIC_BASE: u32 = 0x1B;
    let msr = Msr::new(IA32_APIC_BASE);
    let lapic_base = msr.read();

    let lapic_phys_addr = lapic_base & 0xFFFFF000;

    //we're using the hhdm, since we know it's a usable space (thanks limine)
    let hhdm_response = HHDM_REQUEST.get_response().unwrap();
    let offset = hhdm_response.offset();

    let lapic_addr = lapic_phys_addr + offset;

    let reg_ptr = (lapic_addr + 0x20) as *const u32;

    // Volatile read, since this is MMIO
    let value = reg_ptr.read_volatile();

    // Bits 31:24 contain the ID
    let lapic_id = value >> 24;

    let mp_response = MP_REQUEST.get_response().unwrap();
    let bsp_lapic_id = mp_response.bsp_lapic_id();

    if lapic_id != bsp_lapic_id {
        hcf();
    }
}
