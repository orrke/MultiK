use crate::kernel::hcf;

pub static mut AP_ENTRY: APBootstrap = APBootstrap::new();

pub unsafe fn ap_entry_point() -> ! {
    hcf();
}

#[repr(align(4096))]
pub struct APBootstrap {
    code: [u8; 4096],
}

impl APBootstrap {
    pub const fn new() -> Self {
        APBootstrap {
            code: [0; 4096],
        }
    }

    pub fn init(&self) {
        //get our entry point
        let entry_offset = ap_entry_point as u64;

        //get our own struct as a mut (terribly unsafe, but I don't have another way to do it cleanly)
        let mut self_mut = unsafe { self.as_mut() };

        self_mut.code[0] = 0x48; //mov
        self_mut.code[1] = 0xB8; //rax (destination)
        self_mut.code[2..6].copy_from_slice(&entry_offset.to_le_bytes()); //address to our entry

        self_mut.code[7] = 0xFF; //jmp
        self_mut.code[8] = 0xE0; //rax (source)
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn as_mut(&self) -> Self {
        //This is messy, but it's safe to do
        //Because it's only accessed once (by the BSP)
        //At a time when multithreading isn't a thing yet
        //So, technically, I'm good doing this
        (self as *const _ as *mut Self).read()
    }
}
