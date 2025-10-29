use spin::Mutex;
use crate::kernel::cpu_drv::interrupts::gdt::{standard_gdt_setup, GlobalDescriptorTable, SegmentSelector, TSS};
use crate::kernel::cpu_drv::interrupts::interrupt::{idt_setup, init_pics, PICS};
use crate::kernel::cpu_drv::interrupts::idt::InterruptDescriptorTable;

pub static KERNEL_CODE_SELECTOR: SegmentSelector = SegmentSelector::new(1, 0);
pub static KERNEL_DATA_SELECTOR: SegmentSelector = SegmentSelector::new(2, 0);
pub static TSS_SELECTOR:         SegmentSelector = SegmentSelector::new(5, 0);

pub static IDT: Mutex<InterruptDescriptorTable> = Mutex::new(InterruptDescriptorTable::new());

pub static DOUBLE_FAULT_STACK_SIZE: usize = 4096; //in bytes
pub static mut DOUBLE_FAULT_STACK: [u8; DOUBLE_FAULT_STACK_SIZE] = [0; DOUBLE_FAULT_STACK_SIZE];

pub mod interrupt;
pub mod gdt;
pub mod idt;

pub fn interrupts_setup() {
    unsafe {
        PICS.lock().initialize();
    }
    x86_64::instructions::interrupts::enable();

    //create the TSS
    let mut tss = TSS::new();
    tss.init();

    //create the GDT
    let mut gdt = GlobalDescriptorTable::new();
    //initial setup for the gdt
    standard_gdt_setup(&mut gdt, &tss);
    //initialize the gdt
    gdt.init();

    //update all the registers
    unsafe {
        gdt.load_cs(&KERNEL_CODE_SELECTOR);
        gdt.load_data_regs(&KERNEL_DATA_SELECTOR);
        tss.ltr(&TSS_SELECTOR);
    }

    let mut idt_guard = IDT.lock();
    idt_setup(&mut idt_guard);
    idt_guard.init();

    init_pics();

    x86_64::instructions::interrupts::enable();
}
