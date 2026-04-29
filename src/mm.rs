#[repr(align(4096))]
struct PageTable {
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