//! Attempts to allocate all free memory and then tests how contiguous that memory is by reading
//! the pagemap.

use std::{collections::BTreeMap, io};

use frag_test::{PageMap, PAGE_SIZE};

const PAGEMAP: &str = "/proc/self/pagemap";

const MMAP_ADDR: u64 = 0x7f5707200000;

fn main() -> io::Result<()> {
    // Check how much memory is available.
    let avail_bytes = available_bytes()?;

    println!(
        "Available bytes: {}B (~{}GB)",
        avail_bytes,
        avail_bytes >> 30
    );

    // Allocate that much memory.
    mmap_populate(avail_bytes);

    // Read pagemap to see how contiguous our memory is.
    process_allocated_mem(avail_bytes)
}

fn available_bytes() -> io::Result<usize> {
    let meminfo = std::fs::read_to_string("/proc/meminfo")?;

    for line in meminfo.lines() {
        if line.contains("MemAvailable") {
            return Ok(line
                .split_whitespace()
                .skip(1)
                .next()
                .unwrap()
                .trim()
                .parse::<usize>()
                .unwrap()
                << 10);
        }
    }

    return Err(io::Error::new(
        io::ErrorKind::UnexpectedEof,
        "No MemAvailable line...",
    ));
}

fn mmap_populate(bytes: usize) {
    use libc::{
        c_void, mmap, MAP_ANONYMOUS, MAP_FIXED, MAP_POPULATE, MAP_PRIVATE, PROT_READ, PROT_WRITE,
    };

    assert!(bytes % PAGE_SIZE == 0);

    unsafe {
        let addr = mmap(
            MMAP_ADDR as *mut c_void,
            bytes as u64,
            PROT_READ | PROT_WRITE,
            MAP_ANONYMOUS | MAP_PRIVATE | MAP_FIXED | MAP_POPULATE,
            -1,
            0,
        );
        assert_eq!(addr, MMAP_ADDR as *mut c_void);
    }
}

fn process_allocated_mem(bytes: usize) -> io::Result<()> {
    let mut pagemap = PageMap::new(std::fs::File::open(PAGEMAP)?);

    let mut contig = Vec::new();
    let mut start = 0;
    let mut prev = 0;

    for page in pagemap
        .get_by_range(MMAP_ADDR, MMAP_ADDR + (bytes as u64))?
        .into_iter()
    {
        if page.pfn() == 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Need to run as root to see PFNs",
            ));
        }

        if page.pfn() != prev + 1 {
            contig.push((start, prev));
            start = page.pfn();
        }

        prev = page.pfn();
    }

    //println!("{:?}", contig);

    println!("Number of contiguous regions: {}\n", contig.len());
    //println!(
    //    "{:?}",
    //    contig.iter().map(|(s, e)| e - s + 1).collect::<Vec<_>>()
    //);
    println!(
        "{:#?}",
        categorize(&contig.iter().map(|(s, e)| e - s + 1).collect::<Vec<_>>())
    );

    Ok(())
}

fn categorize(contig: &[u64]) -> BTreeMap<u64, usize> {
    let mut categorized = BTreeMap::new();

    for range in contig.iter() {
        *categorized.entry(*range).or_insert(0) += 1;
    }

    categorized
}
