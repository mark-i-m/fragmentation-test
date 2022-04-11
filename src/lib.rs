//! A utility for reading `/proc/[pid]/pagemap` to produce a profile for eager paging.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;

pub const VSYSCALL_SECTION_START: u64 = 0xffffffffff600000;

// A bunch of constants from Linux 4.15 (probably valid on other versions)...
pub const PAGEMAP_PRESENT_MASK: u64 = 1 << 63;
pub const PAGEMAP_SWAP_MASK: u64 = 1 << 62;
pub const PAGEMAP_FILE_MASK: u64 = 1 << 61;
pub const PAGEMAP_EXCLUSIVE_MASK: u64 = 1 << 56;
pub const PAGEMAP_SOFT_DIRTY_MASK: u64 = 1 << 55;
pub const PAGEMAP_PFN_MASK: u64 = (1 << 55) - 1; // bits 54:0

/// The data for a single page of virtual memory in `/proc/[pid]/pagemap`. That file is basically a
/// huge array of these values.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(C)]
pub struct SinglePageData(u64);

impl SinglePageData {
    /// Is the page present in RAM?
    pub fn present(self) -> bool {
        self.0 & PAGEMAP_PRESENT_MASK != 0
    }

    /// Is the page swapped out?
    pub fn swap(self) -> bool {
        self.0 & PAGEMAP_SWAP_MASK != 0
    }

    /// Is the page file-backed/shared-anonymous?
    pub fn file_backed(self) -> bool {
        self.0 & PAGEMAP_FILE_MASK != 0
    }

    /// Is the page mapped exclusively (i.e., by exactly one user)?
    pub fn exclusive(self) -> bool {
        self.0 & PAGEMAP_EXCLUSIVE_MASK != 0
    }

    /// Can be used to manually implement dirty bits in software (separately from the
    /// hardware-based implementation).
    pub fn soft_dirty(self) -> bool {
        self.0 & PAGEMAP_SOFT_DIRTY_MASK != 0
    }

    /// The page frame number of the physical page backing this virtual page if the page is present
    /// in RAM. Otherwise, if the page is swapped out, then bits 4-0 indicate swap type (i.e.,
    /// which swap space), and bits 54-5 indicate the swap slot on the swap space.
    pub fn pfn(self) -> u64 {
        self.0 & PAGEMAP_PFN_MASK
    }
}

impl std::fmt::Display for SinglePageData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}{}{}{} {}",
            if self.present() { "P" } else { "-" },
            if self.swap() { "S" } else { "-" },
            if self.file_backed() { "F" } else { "-" },
            if self.exclusive() { "X" } else { "-" },
            if self.soft_dirty() { "D" } else { "-" },
            self.pfn()
        )
    }
}

/// The contents of `/proc/[pid]/pagemap` in a seekable way.
pub struct PageMap {
    file: BufReader<File>,
}

impl PageMap {
    pub fn new(file: File) -> Self {
        PageMap {
            file: BufReader::new(file),
        }
    }

    /// Get the `SinglePageData` for the page starting at the given address.
    pub fn get_by_vaddr(&mut self, vaddr: u64) -> std::io::Result<SinglePageData> {
        // Sanity check
        assert!(vaddr & (PAGE_SIZE as u64 - 1) == 0);

        let offset = (vaddr >> PAGE_SHIFT) * 8;

        // Read data from file...
        let single_page = {
            let mut data = [0u8; 8];
            self.file.seek(SeekFrom::Start(offset))?;
            self.file.read_exact(&mut data)?;

            unsafe { std::mem::transmute(data) }
        };

        Ok(single_page)
    }

    /// Get the `SinglePageData` for the pages in the given range of addresses.
    /// `start` is inclusive, `end` is exclusive.
    pub fn get_by_range(&mut self, start: u64, end: u64) -> std::io::Result<Vec<SinglePageData>> {
        assert!(
            (end - start) % (PAGE_SIZE as u64) == 0,
            "Range is not page-sized"
        );
        assert!(start % (PAGE_SIZE as u64) == 0, "Range is not page-aligned");

        let offset = (start >> PAGE_SHIFT) * 8;
        let len = ((end - start) >> PAGE_SHIFT) * 8;

        let mut data = vec![0; len as usize];
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut data)?;

        let data = {
            let mut data = std::mem::ManuallyDrop::new(data);
            let ptr = data.as_mut_ptr() as *mut SinglePageData;
            let len = data.len() / 8;
            let cap = data.capacity() / 8;
            unsafe { Vec::from_raw_parts(ptr, len, cap) }
        };

        Ok(data)
    }
}
