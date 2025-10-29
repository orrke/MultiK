use core::arch::asm;
use super::{DOUBLE_FAULT_STACK, DOUBLE_FAULT_STACK_SIZE};

pub static mut GDT_ENTRIES: [u64; 7] = [0u64; 7]; //first 4 entries are 64 bits, last one is 128 bits

#[repr(transparent)]
pub struct SegmentSelector(pub u16);

impl SegmentSelector {
    pub const fn new(index: u16, rpl: u16) -> SegmentSelector {
        SegmentSelector(index << 3 | (rpl & 0b11))
    }
}

#[repr(C, packed)]
pub struct GDTPointer {
    pub limit: u16,
    pub base: u64,
}

impl GDTPointer {
    pub const fn new(base: u64) -> GDTPointer {
        GDTPointer {
            limit: (size_of::<[u64; 7]>() - 1) as u16, // Correct limit
            base,
        }
    }
}

pub struct GlobalDescriptorTable {
    pub limit: u16, //size in bytes
    pub base: u64, //base pointer
    entries: [Option<GDTEntry>; 5], //
    tss_entries: [Option<TSSDescriptor>; 1]
}

impl GlobalDescriptorTable {
    pub const fn new() -> GlobalDescriptorTable {
        GlobalDescriptorTable {
            limit: 0,
            base: 0, //we don't know it yet
            entries: [None, None, None, None, None], //might seem weird, ut that way I don't have to implement the copy trait for GDTEntry
            tss_entries: [None]
        }
    }

    pub fn init(&mut self) {
        let  gdt_slice = unsafe { &raw mut GDT_ENTRIES };


        for (index, entry_opt) in self.entries.iter_mut().enumerate() {
            if let Some(entry) = entry_opt {
                unsafe {
                    (*gdt_slice)[index] = entry.get_content();
                }
            }
        }

        if let Some(entry) = self.tss_entries[0].as_mut() {
            let data = entry.get_content();
            let data_low = data.0;
            let data_high = data.1;
            unsafe {
                (*gdt_slice)[5] = data_low;
                (*gdt_slice)[6] = data_high;
            }
        }

        self.base = gdt_slice as *mut u8 as u64;
        self.limit = (7 * 8) as u16;

        let pointer = GDTPointer::new(self.base);

        //load the GDT into the CPU's register

        unsafe { self.lgdt(&pointer) }
    }

    pub fn add_entry(&mut self, entry: GDTEntry, index: usize) {
        self.entries[index] = Some(entry);
    }

    pub fn add_tss_entry(&mut self, entry: TSSDescriptor, index: usize) {
        self.tss_entries[index] = Some(entry);
    }

    unsafe fn lgdt(&self, pointer: &GDTPointer) {
        unsafe {
            asm!(
                "lgdt [{}]",
                in(reg) pointer,
                options(nostack, preserves_flags)
            )
        }
    }

    pub unsafe fn load_cs(&self, selector: &SegmentSelector) {
        unsafe {
            asm!(
            "push {selector}",
            "lea {tmp}, [rip + 2f]",
            "push {tmp}",
            "retfq",
            "2:",
            selector = in(reg) selector.0 as u64,
            tmp = lateout(reg) _,
            options(preserves_flags),
            )
        }
    }

    pub unsafe fn load_data_regs(&self, selector: &SegmentSelector) {
        unsafe {
            asm!(
            "mov ss, {sel:x}",
            "mov ds, {sel:x}",
            "mov es, {sel:x}",
            "mov fs, {sel:x}",
            "mov gs, {sel:x}",
            sel = in(reg) selector.0,
            options(nostack, preserves_flags)
            );
        }
    }
}

pub struct GDTEntry {
    base: u32,
    limit: u32,

    is_executable: bool,

    privilege_level: u8,

    present: bool,

    is_64bit: bool,

    granularity: bool,

    conforming: bool,

    writable: bool,
}

impl GDTEntry {
    pub fn new(
        base: u32,
        limit: u32,
        is_executable: bool,
        privilege_level: u8,
        present: bool,
        is_64bit: bool,
        granularity: bool,
        conforming: bool,
        writable: bool,
    ) -> GDTEntry {
        GDTEntry {
            base,
            limit,
            is_executable,
            privilege_level,
            present,
            is_64bit,
            granularity,
            conforming,
            writable,
        }
    }

    fn get_content(&self) -> u64 {
        let mut data: u64 = 0;
        data |= (self.limit as u64) & 0x_0000_FFFF; // bits 0 - 15
        data |= ((self.base as u64) & 0x_00FF_FFFF) << 16; //bits 16 - 39

        //Access byte, bits 40 - 47
        let mut access_byte = 0u8;
        access_byte |= (self.present as u8) << 7; //present bit
        access_byte |= self.privilege_level << 5; //kernel privilege bits
        access_byte |= 1u8 << 4; //type bit, for now it's gonna be 1
        access_byte |= (self.is_executable as u8) << 3; //segment/data bit
        access_byte |= (self.conforming as u8) << 2; //conforming bit
        access_byte |= (self.writable as u8) << 1; //rw bit
        //access bit is always 0 initially
        data |= (access_byte as u64) << 40;
        //end of access byte

        data |= ((self.limit as u64) & 0x_000F_0000) << 32; //bits 48 - 51

        //flag bits 52 - 55
        let mut flags = 0u8; //first bit always 0 (bit 52)
        if self.is_executable {
            flags |= (self.is_64bit as u8) << 1;
        } else {
            flags |= 1 << 2;
        }
        flags |= (self.granularity as u8) << 3; //bit 55
        data |= (flags as u64) << 52; // bits 52 - 55
        //end of flag bits

        data |= ((self.base as u64) & 0x_FF00_0000) << 32; //bits 46-63

        data
    }
}

//read as GDT TSS Entry
pub struct TSSDescriptor {
    limit: u32,
    base: u64,
    access_byte: u8,
    flags_nibble: u8,
}

impl TSSDescriptor {
    fn new() -> TSSDescriptor {
        TSSDescriptor {
            limit: 103,
            base: 0, //unknown
            access_byte: 0x89,
            flags_nibble: 0,
        }
    }

    fn get_content(&self) -> (u64, u64) {
        let base = self.base;   // This is your correct 0xffff80007ff90c40
        let limit = self.limit; // This should be 103

        // --- Build the Low u64 (GDT slot 1) ---
        let mut low: u64 = 0;

        // Bits 0-15: Limit[15:0]
        low |= limit as u64 & 0xFFFF; // Correctly gets the low 16 bits of the limit

        // Bits 16-39: Base[23:0]
        low |= (base & 0xFFFFFF) << 16; // Correctly gets the low 24 bits of the base

        // Bits 40-47: Access Byte (should be 0x89)
        low |= (self.access_byte as u64) << 40;

        // Bits 48-51: Limit[19:16]
        low |= (((limit >> 16) & 0xF) as u64) << 48; // Correctly gets the high 4 bits of the limit

        // Bits 52-55: Flags Nibble (should be 0x0)
        low |= (self.flags_nibble as u64) << 52;

        // Bits 56-63: Base[31:24]
        low |= ((base >> 24) & 0xFF) << 56; // Correctly gets the 3rd byte of the base

        // --- Build the High u64 (GDT slot 2) ---
        let mut high: u64 = 0;

        // Bits 0-31 of this u64 correspond to bits 64-95 of the descriptor.
        // This part holds Base[63:32].
        high |= (base >> 32) & 0xFFFFFFFF; // Correctly gets the high 32 bits of the base

        (low, high)
    }
}

#[repr(C, packed(4))]
pub struct TSS {
    _reserved1: u32,
    pub pst: [u64; 3],
    _reserved2: u64,
    pub ist: [u64; 7],
    _reserved3: u64,
    _reserved4: u16,
    pub io_map_base: u16,
}

impl TSS {
    pub fn new() -> TSS {
        TSS {
            pst: [0; 3],
            ist: [0; 7],

            io_map_base: core::mem::size_of::<TSS>() as u16,

            _reserved1: 0,
            _reserved2: 0,
            _reserved3: 0,
            _reserved4: 0,
        }
    }

    pub fn init(&mut self) {
        let df_stack = unsafe { &raw mut DOUBLE_FAULT_STACK };
        let stack_start = df_stack as *const _ as u64;
        let stack_end = stack_start + DOUBLE_FAULT_STACK_SIZE as u64;

        self.ist[0] = stack_end; //stack goes down
    }

    pub unsafe fn ltr(&self, code_selector: &SegmentSelector) {
        unsafe {
            asm!(
            "ltr {selector:x}",
            selector = in(reg) code_selector.0,
            options(nostack, preserves_flags)
            )
        }
    }
}

pub fn standard_gdt_setup(gdt: &mut GlobalDescriptorTable, tss: &TSS) {
    let entry1 = GDTEntry::new(0, 0xFFFFF, true, 0, true, true, true, false, true);
    gdt.add_entry(entry1, 1);

    let entry2 = GDTEntry::new(0, 0xFFFFF, false, 0, true, false, true, false, true);
    gdt.add_entry(entry2, 2);

    let entry3 = GDTEntry::new(0, 0xFFFFF, true, 3, true, true, true, false, true);
    gdt.add_entry(entry3, 3);

    let entry4 = GDTEntry::new(0, 0xFFFFF, false, 3, true, false, true, false, true);
    gdt.add_entry(entry4, 4);

    let tss_pointer = tss as *const _ as u64;
    let mut entry5 = TSSDescriptor::new();
    entry5.base = tss_pointer;
    gdt.add_tss_entry(entry5, 0);
}
