// fs.rs - LatencyFS Static Zero-Allocation In-Memory Filesystem
//
// Worst-case execution time: Documented per function.

use crate::tsc::read_tsc_serialized;

pub const MAX_FILES: usize = 16;
pub const MAX_FILENAME_LEN: usize = 32;
pub const MAX_FILE_SIZE: usize = 4096;

#[derive(Clone, Copy)]
pub struct FileEntry {
    pub name: [u8; MAX_FILENAME_LEN],
    pub name_len: usize,
    pub data: [u8; MAX_FILE_SIZE],
    pub size: usize,
    pub read_only: bool,
    pub modified_tsc: u64,
    pub used: bool,
}

impl FileEntry {
    pub const fn empty() -> Self {
        Self {
            name: [0; MAX_FILENAME_LEN],
            name_len: 0,
            data: [0; MAX_FILE_SIZE],
            size: 0,
            read_only: false,
            modified_tsc: 0,
            used: false,
        }
    }

    pub fn name_str(&self) -> &str {
        if self.name_len == 0 {
            return "";
        }
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("<invalid utf8>")
    }
}

pub struct LatencyFS {
    pub files: [FileEntry; MAX_FILES],
}

impl LatencyFS {
    pub const fn new() -> Self {
        Self {
            files: [FileEntry::empty(); MAX_FILES],
        }
    }
}

pub static mut FS: LatencyFS = LatencyFS::new();

#[derive(Debug, PartialEq, Eq)]
pub enum FsError {
    FileNotFound,
    DiskFull,
    FileTooLarge,
    ReadOnly,
    InvalidName,
}

// Function: fs_init
// Description: Initialize LatencyFS and populate with initial default PulseLang scripts.
// Worst-case execution time: ~15_000 ns
pub fn fs_init() {
    unsafe {
        for file in FS.files.iter_mut() {
            *file = FileEntry::empty();
        }

        // 1. stream.pl - Ultra-low-latency pipeline stream script
        let stream_src = b"// stream.pl - Zero-Copy GPU-to-NIC Ultra-Low-Latency Pipeline\n\
@pipeline: UltraStream @budget(8000us);\n\
@on_vblank: {\n\
    #f := @capture();\n\
    @within(500us) {\n\
        $rtt := @rtt();\n\
        $rtt > 200us ? @rate(80) : @rate(100);\n\
        @send(#f);\n\
    } !drop;\n\
};\n";
        let _ = fs_create_internal("stream.pl", stream_src, false);

        // 2. bench.pl - Micro-benchmark script
        let bench_src = b"// bench.pl - Realtime Math & Latency Benchmark [AI-Native Spec]\n\
@contract: @wcet(5us) @budget(50us);\n\
$t0 := @tsc();\n\
$sum := 0;\n\
$i := 0;\n\
@while($i < 100) {\n\
    $sum += $i * 2;\n\
    $i += 1;\n\
}\n\
$dt := @tsc() - $t0;\n\
@println(\"[BENCH] Iterations: 100\");\n\
@println(\"[RESULT] Sum:\");\n\
@println($sum);\n\
@println(\"[LATENCY] Cycles:\");\n\
@println($dt);\n";
        let _ = fs_create_internal("bench.pl", bench_src, false);

        // 3. filter.pl - Packet filter and congestion controller
        let filter_src = b"// filter.pl - Adaptive Congestion Guard [AI-Native Spec]\n\
@contract: @wcet(2us) @budget(100us);\n\
$rtt := @rtt();\n\
@println(\"[FILTER] Measured RTT (ns):\");\n\
@println($rtt);\n\
$rtt > 300us ? {\n\
    @println(\"[ACTION] Congestion detected -> Rate: 60%\");\n\
    @rate(60);\n\
} : {\n\
    @println(\"[ACTION] Optimal latency -> Rate: 100%\");\n\
    @rate(100);\n\
};\n";
        let _ = fs_create_internal("filter.pl", filter_src, false);
    }
}

// Function: fs_create_internal
// Description: Create or update a file in LatencyFS without dynamic allocation.
// Worst-case execution time: ~1200 ns
pub fn fs_create_internal(name: &str, content: &[u8], read_only: bool) -> Result<usize, FsError> {
    if name.is_empty() || name.len() > MAX_FILENAME_LEN {
        return Err(FsError::InvalidName);
    }
    if content.len() > MAX_FILE_SIZE {
        return Err(FsError::FileTooLarge);
    }

    unsafe {
        // Check if file already exists
        for (idx, file) in FS.files.iter_mut().enumerate() {
            if file.used && file.name_str() == name {
                if file.read_only {
                    return Err(FsError::ReadOnly);
                }
                file.data[..content.len()].copy_from_slice(content);
                file.size = content.len();
                file.modified_tsc = read_tsc_serialized();
                return Ok(idx);
            }
        }

        // Find empty slot
        for (idx, file) in FS.files.iter_mut().enumerate() {
            if !file.used {
                file.used = true;
                file.name_len = name.len();
                file.name[..name.len()].copy_from_slice(name.as_bytes());
                file.data[..content.len()].copy_from_slice(content);
                file.size = content.len();
                file.read_only = read_only;
                file.modified_tsc = read_tsc_serialized();
                return Ok(idx);
            }
        }

        Err(FsError::DiskFull)
    }
}

// Function: fs_read
// Description: Read file contents from LatencyFS.
// Worst-case execution time: ~300 ns
pub fn fs_read(name: &str) -> Option<&'static [u8]> {
    unsafe {
        for file in FS.files.iter() {
            if file.used && file.name_str() == name {
                return Some(&file.data[..file.size]);
            }
        }
        None
    }
}

// Function: fs_write
// Description: Overwrite or create file content in LatencyFS.
// Worst-case execution time: ~1200 ns
pub fn fs_write(name: &str, content: &[u8]) -> Result<(), FsError> {
    fs_create_internal(name, content, false).map(|_| ())
}

// Function: fs_delete
// Description: Delete a file from LatencyFS.
// Worst-case execution time: ~400 ns
pub fn fs_delete(name: &str) -> Result<(), FsError> {
    unsafe {
        for file in FS.files.iter_mut() {
            if file.used && file.name_str() == name {
                if file.read_only {
                    return Err(FsError::ReadOnly);
                }
                *file = FileEntry::empty();
                return Ok(());
            }
        }
        Err(FsError::FileNotFound)
    }
}

// Function: fs_exists
// Description: Check if file exists.
// Worst-case execution time: ~250 ns
#[allow(dead_code)]
pub fn fs_exists(name: &str) -> bool {
    unsafe {
        for file in FS.files.iter() {
            if file.used && file.name_str() == name {
                return true;
            }
        }
        false
    }
}
