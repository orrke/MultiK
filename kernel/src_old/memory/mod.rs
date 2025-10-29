use core::alloc::{GlobalAlloc, Layout};
use core::arch::asm;
use core::cell::{OnceCell, UnsafeCell};
use core::ops::DerefMut;
use lazy_static::lazy_static;
use limine::memory_map::{Entry};
use limine::response::{ExecutableAddressResponse, HhdmResponse};
use spin::{Mutex, MutexGuard};
use crate::{println, EXEC_REQUEST, HHDM_REQUEST, MEMMAP_REQUEST};
use crate::memory::allocator::{FlatAlloc, MemAlloc, PHYSICAL_MEMORY_ALLOCATOR};
use crate::memory::slub::SlubAllocator;
use crate::memory::page_alloc::FREE_PAGE_LIST;

use crate::memory::page_structs::BitmapState::ALLOCATED;

pub mod heap;
mod page_alloc;
mod page_structs;
mod vmem;
mod allocator;
mod slub;

static ALLOCATOR: Allocator = Allocator::new();

#[global_allocator]
pub static GLOBAL: Dummy = Dummy;

pub struct Dummy;

lazy_static!(
    pub static ref KERNEL_OFFSET: u64 = get_kernel_slide();
);

unsafe extern "C" {
    static mut _kernel_end: u64;
}

fn init_hhdm() -> &'static HhdmResponse {
    HHDM_REQUEST.get_response().expect("No HHDM")
}

fn init_mmap() -> &'static [&'static Entry] {
    MEMMAP_REQUEST.get_response().expect("No MEMMAP").entries()
}

fn init_exec() -> &'static ExecutableAddressResponse {
    EXEC_REQUEST.get_response().expect("No EXEC")
}

fn get_kernel_slide() -> u64 {
    let exec = init_exec();
    exec.virtual_base() - exec.physical_base()
}

#[inline]
pub fn phys_to_hhdm(paddr: u64, offset: u64) -> u64 {
    paddr + offset
}

#[inline]
pub fn hhdm_to_phys(vaddr: u64, offset: u64) -> u64 {
    vaddr - offset
}

#[allow(irrefutable_let_patterns, unused)]
pub fn init_kernel_allocators() {
    println!("Beginning initialization of kernel allocators");

    let mut flat_allocator = FlatAlloc::new();

    let kernel_end_addr = unsafe { _kernel_end };

    let mmap = init_mmap();

    let exec = init_exec();
    let virtual_base = exec.virtual_base();
    let phys_base = exec.physical_base();

    let kernel_slide = virtual_base - phys_base;

    let kernel_phys_addr = kernel_end_addr - kernel_slide;

    println!("kernel end phys: {:#x}", kernel_phys_addr);

    let mmap_length = mmap.len();

    let hhdm_offset = init_hhdm().offset();

    let mut best_region: Option<&Entry> = None;
    for (i, entry) in mmap.iter().enumerate() {
        if entry.entry_type == limine::memory_map::EntryType::USABLE {
            // Skip if the region overlaps with HHDM
            if entry.base >= kernel_phys_addr && entry.base + entry.length <= hhdm_offset {
                // Region is usable, above kernel_phys_addr, and does not overlap with HHDM
                if best_region.is_none() || entry.length > best_region.unwrap().length {
                    best_region = Some(entry);
                }
            }
        }
    }


    let heap_region = best_region.expect("No region suitable for the heap");
    let heap_start = heap_region.base + hhdm_offset;

    println!("Initializing flat heap...");

    flat_allocator.init(heap_start);

    println!("Flat heap initialized");

    if let mut free_list = FREE_PAGE_LIST.lock() {
        let len = free_list.len();

        for i in 0..len {
            unsafe { free_list.add_single_page(flat_allocator.alloc() as u64); };
        }
    }

    unsafe {
        println!("Initialized the free page list");

        ALLOCATOR.get_mut().init();

        println!("Initialized both small and large allocators");

        let mut large_alloc = ALLOCATOR.get_mut().inner_large.get_mut();

        let mut vma = unsafe { large_alloc.get_vma().read() };
        let mut pt_walker = unsafe { large_alloc.get_pt_walker().read() };

        println!("Got VMA and walker");
        //first things first, we map the kernel addresses
        let kernel_phys = exec.physical_base();
        let kernel_virt = exec.virtual_base();


        let kernel_end = _kernel_end;

        let kernel_size = kernel_end - kernel_virt;

        println!("Kernel virtual address: {:#x}", kernel_virt);
        println!("kernel end virtual address: {:#x}", kernel_virt + kernel_size);

        println!("Before first vma update: {:#x}", kernel_size);

        vma.update_state(kernel_size as usize, kernel_virt, true); //update the state for the kernel code segment
        println!("After first vma update");
        pt_walker.setup_pages(kernel_virt as *mut u8, kernel_phys, kernel_size as usize, true, false, false);

        println!("After first pt update");
        println!("After first pt update");
        println!("After first pt update");
        println!("After first pt update");
        println!("After first pt update");
        println!("After first pt update");
        println!("After first pt update");
        println!("After first pt update");
        println!("After first pt update");

        //map a higher half direct map for our buddy to use.
        let mut highest_phys_addr = 0;
        for entry in init_mmap() {
            let end = entry.base + entry.length;
            if end > highest_phys_addr {
                highest_phys_addr = end;
            }
        }

        vma.update_state(highest_phys_addr as usize, highest_phys_addr, true);
        pt_walker.setup_pages(hhdm_offset as *mut u8, 0u64, highest_phys_addr as usize, true, false, false);

        //finally, map our temporary heap.
        let heap_size = flat_allocator.get_size();
        let heap_start = _kernel_end + hhdm_offset;

        vma.update_state(heap_size, heap_start, true);
        pt_walker.setup_pages(heap_start as *mut u8, _kernel_end - *KERNEL_OFFSET, heap_size, true, false, false);

        let new_hhdm = vma.alloc(Layout::from_size_align_unchecked(highest_phys_addr as usize, 0x1000), true);

        println!("Atempting switch");
        println!("Atempting switch");
        println!("Atempting switch");
        println!("Atempting switch");
        println!("Atempting switch");
        println!("Atempting switch");
        println!("Atempting switch");

        //and now, we can switch to our new page table wihout any issues, normally.
        switch_to_new_table(pt_walker.get_pml4_pointer());

        println!("Switch succesful");
        println!("NOW RUNNING ON NEW TABLE!!!!!!!!!!!!!!!!");

        let bitmap_size = highest_phys_addr / 4096;
        let bitmap_start_phys = highest_phys_addr - 4096;
        let bitmap_start_virt = hhdm_offset + bitmap_start_phys;

        //now that we're on the new table, we can finally finish the initialization with the buddy allocator
        if let mut phys_alloc = PHYSICAL_MEMORY_ALLOCATOR.lock() {
            phys_alloc.init(new_hhdm as u64, highest_phys_addr as usize, bitmap_start_virt, bitmap_size);

            //mark everything we did earlier as allocated
            phys_alloc.mark_as_allocated(bitmap_start_phys, bitmap_size as usize); //bitmap
            //phys_alloc.mark_as_allocated(hhdm_offset, highest_phys_addr as usize); //I honestly don't remember why I put this there, seems kinda dumb
            phys_alloc.mark_as_allocated(kernel_phys_addr, kernel_size as usize); //kernel
            phys_alloc.mark_as_allocated(heap_start, heap_size); //heap
        }
    }
}


#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn switch_to_new_table(new_pml4: *mut u8) {
    let slide = get_kernel_slide();
    let physical_pml4 = new_pml4 as u64 - slide;

    asm!(
        "mov cr3, {0}",
        in(reg) physical_pml4,
        options(nostack, preserves_flags)
    );
}

struct Allocator {
    inner_large: Mutex<MemAlloc>,
    inner_small: Mutex<SlubAllocator>,
    is_multithreaded: bool,
}

#[allow(unsafe_op_in_unsafe_fn)]
impl Allocator {
    pub const fn new() -> Self {
        Allocator {
            inner_large: Mutex::new(MemAlloc::new()),
            inner_small: Mutex::new(SlubAllocator::new()),
            is_multithreaded: false,
        }
    }

    pub fn init(&mut self) {
        let large_alloc = &mut *self.inner_large.get_mut();
        (&mut *self.inner_small.get_mut()).init(large_alloc);
        large_alloc.init();
    }

    #[allow(unused_mut)]
    pub unsafe fn get_mut(&self) -> &mut Self {
        let mut ptr: *mut Allocator = self as *const _ as *mut Allocator;
        &mut (*ptr)
    }

    pub unsafe fn alloc_single(&mut self, layout: Layout) -> *mut u8 {
        if layout.size() < 4096 {
            let guard = self.inner_small.get_mut();

            guard.allocate(layout)
        } else {
            let guard = self.inner_large.get_mut();

            guard.alloc(layout)
        }
    }

    pub unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() < 4096 {
            let mut guard = self.inner_small.lock();

            guard.allocate(layout)
        } else {
            let mut guard = self.inner_large.lock();

            guard.alloc(layout)
        }
    }

    pub unsafe fn dealloc_single(&mut self, ptr: *mut u8, layout: Layout) {
        if layout.size() < 4096 {
            let guard = self.inner_small.get_mut();

            guard.deallocate(ptr, layout);
        } else {
            let guard = self.inner_large.get_mut();

            guard.dealloc(ptr, layout);
        }
    }

    pub unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if layout.size() < 4096 {
            let mut guard = self.inner_small.lock();

            guard.deallocate(ptr, layout);
        } else {
            let mut guard = self.inner_large.lock();

            guard.dealloc(ptr, layout);
        }
    }
}

unsafe impl Sync for Allocator {}
unsafe impl Send for Allocator {}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe impl GlobalAlloc for Dummy {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ALLOCATOR.is_multithreaded {
            ALLOCATOR.alloc(layout)
        } else {
            ALLOCATOR.get_mut().alloc_single(layout)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ALLOCATOR.is_multithreaded {
            ALLOCATOR.dealloc(ptr, layout);
        } else {
            ALLOCATOR.get_mut().dealloc_single(ptr, layout);
        }
    }
}
