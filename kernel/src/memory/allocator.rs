use core::alloc::Layout;
use spin::Mutex;
use crate::memory::page_alloc::{BuddyAlloc, FREE_PAGE_LIST};
use crate::memory::vmem::{PageTableWalker, VMA};
use crate::println;

pub static PHYSICAL_MEMORY_ALLOCATOR: Mutex<BuddyAlloc> = Mutex::new(BuddyAlloc::new());

pub struct MemAlloc {
    pub initialized: bool,
    user: bool,
    pt_walker: PageTableWalker,
    vma: VMA,
}

unsafe impl Send for MemAlloc {}
unsafe impl Sync for MemAlloc {}

struct Wrapper<'a, T>(&'a mut T);

impl MemAlloc {
    pub const fn new() -> Self {
        MemAlloc {
            initialized: false,
            user: false,
            pt_walker: PageTableWalker::new(),
            vma: VMA::new(),
        }
    }

    pub fn init(&mut self) {
        println!("Trying to initialize large alloc");

        println!("Trying to init vma");
        self.vma.init();
        println!("trying to initialize pt walker");
        let self_ptr: *mut Self = self;
        unsafe {
            (*self_ptr).pt_walker.init(self); //need to do this otherwise it's a lock mess
        };
        println!("initialized large alloc");

        self.initialized = true;
    }

    pub unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        if self.initialized {
            let virtual_address = self.vma.alloc(layout, !self.user);

            let physical_address = PHYSICAL_MEMORY_ALLOCATOR.lock().alloc(layout.size());

            self.pt_walker.setup_pages(virtual_address, physical_address, layout.size(), true, !self.user, false);

            virtual_address
        } else {
            if layout.size() > 4096 {
                panic!("Can't allocate more than one page at a time when not initialized.");
            }

            FREE_PAGE_LIST.lock().allocate_page(false) as *mut u8
        }
    }

    pub unsafe fn dealloc(&mut self, virtual_address: *mut u8, layout: Layout) {
        self.vma.dealloc(virtual_address, layout, !self.user);

        let physical_address = self.pt_walker.unset_page(virtual_address);

        PHYSICAL_MEMORY_ALLOCATOR.lock().free(physical_address, layout.size());
    }

    pub fn get_pt_walker(&self) -> *mut PageTableWalker {
        &self.pt_walker as *const _ as *mut PageTableWalker
    }

    pub fn get_vma(&self) -> *mut VMA {
        &self.vma as *const _ as *mut VMA
    }
}

pub struct FlatAlloc {
    initial_pointer: u64,
    current_pointer: u64,
}

impl FlatAlloc {
    pub const fn new() -> Self {
        FlatAlloc {
            initial_pointer: 0,
            current_pointer: 0,
        }
    }

    pub fn init(&mut self, start: u64) {
        self.initial_pointer = start;
        self.current_pointer = start;
    }

    pub unsafe fn alloc(&mut self) -> *mut u8 {
        let ptr = self.current_pointer as *mut u8;

        self.current_pointer += 4096;

        ptr
    }

    pub fn get_size(&self) -> usize {
        (self.current_pointer - self.initial_pointer) as usize
    }
}
