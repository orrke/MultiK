use core::arch::asm;
use super::KERNEL_CODE_SELECTOR;

static mut IDT_ENTRIES: [u128; 256] = [0; 256]; //amount of interrupts is 256, and a single entry is 128 bits.

#[repr(C, packed)]
pub struct IdtPointer {
    pub limit: u16,
    pub base: u64,
}

impl IdtPointer {
    pub const fn new() -> IdtPointer {
        IdtPointer {
            limit: 0,
            base: 0,
        }
    }
}

pub struct InterruptDescriptorTable {
    pointer: IdtPointer,
    content: [Option<IDTEntry>; 256]
}

impl InterruptDescriptorTable {
    pub const fn new() -> InterruptDescriptorTable {
        InterruptDescriptorTable {
            pointer: IdtPointer::new(),
            content: [None; 256]
        }
    }

    pub fn init(&mut self) {
        self.pointer.limit = (size_of::<[u128; 256]>() - 1) as u16;

        #[allow(unused_unsafe)]
        let idt_slice = unsafe { &raw mut IDT_ENTRIES };

        self.pointer.base = idt_slice as u64;

        for (i, entry_opt) in self.content.iter().enumerate() {
            if let Some(entry) = entry_opt {
                unsafe { (*idt_slice)[i] = entry.get_content(); }
            }
        }

        unsafe {
            self.lidt();
        };
    }

    pub fn add_entry(
        &mut self,
        interrupt_number: u8, //interrupt numbers can go up to 255, perfectly in our range
        entry: IDTEntry,
    ) {
        self.content[interrupt_number as usize] = Some(entry);
    }

    pub fn set_handler_func(
        &mut self,
        interrupt_number: u8,
        handle_func_ptr: u64,
        level: u8,
        disable_interrupts: bool,
        stack_index: u8,
    ) {
        let mut entry = IDTEntry::new();
        entry.options.present = true;
        entry.options.disable_interrupts = disable_interrupts;
        entry.options.stack_index = stack_index;
        entry.options.privilege_level = level;

        entry.handler_addr = handle_func_ptr;
        entry.code_selector = KERNEL_CODE_SELECTOR.0 as u64;

        self.content[interrupt_number as usize] = Some(entry);
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn lidt(&self) {
        let ptr_ref = &self.pointer;
        let ptr_start: *const u8 = ptr_ref as *const IdtPointer as *const u8;

        asm!(
            "lidt [{}]",
            in(reg) ptr_start,
            options(nomem, nostack, preserves_flags)
        );
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    pub unsafe fn enable_interrupts(&self) {
        asm!("sti");
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    pub unsafe fn disable_interrupts(&self) {
        asm!("cli");
    }
}

pub struct IDTOptions {
    present: bool,
    privilege_level: u8,
    stack_index: u8,
    disable_interrupts: bool,
}

impl IDTOptions {
    pub const fn new() -> IDTOptions {
        IDTOptions {
            present: false,
            privilege_level: 0,
            stack_index: 0,
            disable_interrupts: false,
        }
    }

    fn get_content(&self) -> u128 {
        let mut data: u128 = 0;

        data |= (self.stack_index as u128) << 32;

        //middle is reserved

        let gate_type = if self.disable_interrupts {
            0xE // 0b1110: Interrupt Gate (disables interrupts)
        } else {
            0xF // 0b1111: Trap Gate (does not disable interrupts)
        };
        data |= (gate_type as u128) << 40;

        //bits set to 0
        data |= (self.privilege_level as u128) << 45;
        data |= (self.present as u128) << 47;

        data
    }
}

pub struct IDTEntry {
    handler_addr: u64,
    code_selector: u64,
    options: IDTOptions,
}

impl IDTEntry {
    pub const fn new() -> IDTEntry {
        IDTEntry {
            handler_addr: 0,
            code_selector: 0,
            options: IDTOptions::new(),
        }
    }

    fn get_content(&self) -> u128 {
        let mut data: u128 = 0;

        data |= (self.handler_addr as u128) & 0x_0000_FFFF;

        data |= ((self.code_selector as u128) & 0x_0000_00FF) << 16;

        data |= self.options.get_content(); //already formated correctly

        data |= ((self.handler_addr as u128) & 0x_FFFF_FFFF_FFFF_0000) << 32; // starts at bit 48, we start at bit 16, difference of 32 bits

        //end is reserved

        data
    }
}

impl Copy for IDTOptions {}
impl Clone for IDTOptions {
    fn clone(&self) -> IDTOptions {
        IDTOptions {
            present: self.present,
            privilege_level: self.privilege_level,
            stack_index: self.stack_index,
            disable_interrupts: self.disable_interrupts,
        }
    }
}

impl Copy for IDTEntry {}
impl Clone for IDTEntry {
    fn clone(&self) -> IDTEntry {
        IDTEntry {
            handler_addr: self.handler_addr,
            code_selector: self.code_selector,
            options: self.options.clone(),
        }
    }
}



