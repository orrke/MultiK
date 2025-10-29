use spin::{MutexGuard};
use x86_64::structures::idt::{InterruptStackFrame, PageFaultErrorCode};
use crate::kernel::cpu_drv::interrupts::idt::InterruptDescriptorTable;
use pic8259::ChainedPics;
use crate::kernel::hcf;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard,
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }

    fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

pub static PICS: spin::Mutex<ChainedPics> = spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

pub fn idt_setup(idt_guard: &mut MutexGuard<InterruptDescriptorTable>) {
    idt_guard.set_handler_func(0x8, double_fault_handler as u64, 0, true, 1);
    idt_guard.set_handler_func(InterruptIndex::Timer.as_u8(), timer_interrupt_handler as u64, 0, true, 0);
    //idt_guard.set_handler_func(InterruptIndex::Keyboard.as_u8(), keyboard_interrupt_handler as u64, 0, true, 0);
    idt_guard.set_handler_func(0x13, general_protection_fault_handler as u64, 0, true, 0);
    idt_guard.set_handler_func(0x14, page_fault_handler as u64, 0, true, 0);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    /*
    println!("EXCEPTION: PAGE FAULT");
    println!("Accessed Address: {:?}", Cr2::read());
    println!("Error Code: {:?}", error_code);
    println!("{:#?}", stack_frame);
    */
    hcf();
}

extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    /*
    println!("EXCEPTION: GENERAL PROTECTION FAULT");
    println!("Accessed Address: {:?}", Cr2::read());
    println!("Error Code: {:?}", error_code);
    println!("{:#?}", stack_frame);
    */
    hcf();
}

extern "x86-interrupt" fn timer_interrupt_handler(
    _stack_frame: InterruptStackFrame,
) {
    //print!(".");

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}

/*
extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;

    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };
    crate::task::keyboard::add_scancode(scancode);

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    };
}
*/

pub fn init_pics() {
    unsafe {
        let mut pics = PICS.lock();

        pics.initialize();

        let mut mask1 = pics.read_masks()[0];
        let mut mask2 = pics.read_masks()[1];

        mask1 &= !(1 << (InterruptIndex::Timer as u8 - PIC_1_OFFSET));
        mask1 &= !(1 << (InterruptIndex::Keyboard as u8 - PIC_1_OFFSET));

        pics.write_masks(mask1, mask2);
    }
}
