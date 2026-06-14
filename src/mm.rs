pub mod freelist;

pub use freelist::{init_frame_allocator, alloc_frame, free_frame};

use crate::program::{MAX_USER_CODE_PAGES, USER_CODE_VA, USER_STACK_VA};


#[repr(align(4096))]
pub struct PageTable {
    entries: [usize; 512],
}
impl PageTable {
    const fn new() -> Self {
        Self { entries: [0; 512] }
    }
}

static mut KERNEL_ROOT_PAGE_TABLE: PageTable = PageTable::new();


const PTE_V: usize = 1 << 0;
const PTE_R: usize = 1 << 1;
const PTE_W: usize = 1 << 2;
const PTE_X: usize = 1 << 3;
const PTE_U: usize = 1 << 4;
const PTE_A: usize = 1 << 6;
const PTE_D: usize = 1 << 7;


pub fn map_kernel_gigapages() {
    unsafe {
        // Identity Map 1GB at NULL 0x0 => 0x0
        KERNEL_ROOT_PAGE_TABLE.entries[0] = ((0 >> 12) << 10) |
            PTE_V | PTE_R | PTE_W | PTE_A | PTE_D;
        // Identity Map 1GB at 0x80000000 => 0x80000000
        // We cannot add PTE_U to this page because it is not allowed
        // to execute code on U page in S mode and vice-versa.
        KERNEL_ROOT_PAGE_TABLE.entries[2] = ((0x80000000 >> 12) << 10) |
            PTE_V | PTE_R | PTE_W | PTE_X | PTE_A | PTE_D;
    }
    let _ = PTE_U;  // Slience unused warning

}

pub fn enable_paging() {
    unsafe {
        let root_page_table_addr = core::ptr::addr_of!(KERNEL_ROOT_PAGE_TABLE) as usize;
        let satp = (8usize << 60) | (root_page_table_addr >> 12);
        core::arch::asm!(
            "csrw satp, {satp}",
            "sfence.vma",
            satp = in(reg) satp,
        )
    }
}




pub fn map_page(root: *mut PageTable, va: usize, pa: usize, flags: usize) {
    // va[38:30] -> VPN[2]
    // va[29:21] -> VPN[1]
    // va[20:12] -> VPN[0]
    // va[11:0] -> offset
    let vpn2 = (va >> 30) & 0x1ff;
    let vpn1 = (va >> 21) & 0x1ff;
    let vpn0 = (va >> 12) & 0x1ff;
    let root_entries: &mut [usize; 512] = unsafe { &mut (*root).entries };
    let mut pte = &mut root_entries[vpn2];
    // L2 PTE
    if *pte & PTE_V == 0 {
        // Allocate L1
        let page_table = alloc_frame() as *mut PageTable;
        *pte = ((page_table as usize >> 12) << 10) | PTE_V;
        pte = unsafe { &mut (*page_table).entries[vpn1] };
    } else {
        let page_table: *mut PageTable = ((*pte >> 10) << 12) as *mut PageTable;
        pte = unsafe { &mut (*page_table).entries[vpn1] };
    }

    // L1 PTE
    if *pte & PTE_V == 0 {
        // Allocate L0
        let page_table = alloc_frame() as *mut PageTable;
        *pte = ((page_table as usize >> 12) << 10) | PTE_V;
        pte = unsafe { &mut (*page_table).entries[vpn0] };
    } else {
        let page_table: *mut PageTable = ((*pte >> 10) << 12) as *mut PageTable;
        pte = unsafe { &mut (*page_table).entries[vpn0] };
    }

    *pte = ((pa >> 12) << 10) | flags;

}

/// Build a fresh user address space by copying `program` into private code pages.
pub fn create_user_address_space(program: &[u8], user_stack_pa: usize) -> *mut PageTable {
    let num_pages = (program.len() + 4095) / 4096;
    assert!(
        num_pages <= MAX_USER_CODE_PAGES,
        "user program too large: {} pages",
        num_pages
    );

    let root = alloc_frame() as *mut PageTable;
    unsafe {
        core::ptr::write_bytes(root as *mut u8, 0, 4096);
        // Map kernel gigapages so the kernel can run in the user page table.
        (*root).entries[0] = KERNEL_ROOT_PAGE_TABLE.entries[0];
        (*root).entries[2] = KERNEL_ROOT_PAGE_TABLE.entries[2];
    }

    let code_flags = PTE_V | PTE_U | PTE_R | PTE_X | PTE_A | PTE_D;
    for i in 0..num_pages {
        let page_pa = alloc_frame();
        let dst = page_pa as *mut u8;
        unsafe {
            core::ptr::write_bytes(dst, 0, 4096);
            let src_off = i * 4096;
            let copy_len = core::cmp::min(4096, program.len() - src_off);
            core::ptr::copy_nonoverlapping(program.as_ptr().add(src_off), dst, copy_len);
        }
        map_page(root, USER_CODE_VA + i * 4096, page_pa, code_flags);
    }

    let stack_flags = PTE_V | PTE_U | PTE_R | PTE_W | PTE_A | PTE_D;
    map_page(root, USER_STACK_VA, user_stack_pa, stack_flags);

    root
}

pub fn free_user_page_table(root_pa: usize) {
    // Traversing L2
    let ekernel_addr = core::ptr::addr_of!(crate::ekernel) as usize;
    let is_allocated_frame = |pa: usize| -> bool {
        pa >= ekernel_addr && pa < crate::PHYSICAL_MEMORY_END
    };
    let root = root_pa as *mut PageTable;
    for i in 0..512 {
        if i == 0 || i == 2 {
            // Kernel Gigapage, Skip
            continue;
        }
        let pte = unsafe { (*root).entries[i] };
        if pte & PTE_V == 0 {
            // L1 PTE is not present, Skip
            continue;
        }
        let l1_pa = (pte >> 10) << 12;
        let l1_page_table = l1_pa as *mut PageTable;
        for j in 0..512 {
            let pte = unsafe { (*l1_page_table).entries[j] };
            if pte & PTE_V == 0 {
                // L0 PTE is not present, Skip
                continue;
            }
            let l0_pa = (pte >> 10) << 12;
            let l0_page_table = l0_pa as *mut PageTable;
            for k in 0..512 {
                let pte = unsafe { (*l0_page_table).entries[k] };
                if pte & PTE_V == 0 {
                    // L0 PTE is not present, Skip
                    continue;
                }
                let pa = (pte >> 10) << 12;
                if !is_allocated_frame(pa) {
                    continue;
                }
                free_frame(pa);
            }
            free_frame(l0_pa as usize);
        }
        free_frame(l1_pa as usize);
    }
    free_frame(root_pa as usize);
}

pub fn switch_to_kernel_page_table() {
    enable_paging();
}
