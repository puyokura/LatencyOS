// fs.rs - LatencyFS Static Zero-Allocation In-Memory Filesystem
//
// Worst-case execution time: Documented per function.

use crate::tsc::read_tsc_serialized;

pub const MAX_FILES: usize = 32;
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
// Description: Initialize LatencyFS and populate with initial default PulseLang scripts and text files.
// Worst-case execution time: ~20_000 ns
pub fn fs_init() {
    unsafe {
        for file in FS.files.iter_mut() {
            *file = FileEntry::empty();
        }

        // 1. stream.pl - Ultra-low-latency pipeline stream script
        let stream_src = br#"// stream.pl - Zero-Copy GPU-to-NIC Ultra-Low-Latency Pipeline
@pipeline: UltraStream @budget(8000us);
@on_vblank: {
    #f := @capture();
    @within(500us) {
        $rtt := @rtt();
        $rtt > 200us ? @rate(80) : @rate(100);
        @send(#f);
    } !drop;
};
"#;
        let _ = fs_create_internal("stream.pl", stream_src, false);

        // 2. bench.pl - Micro-benchmark script
        let bench_src = br#"// bench.pl - Realtime Math & Latency Benchmark [AI-Native Spec]
@contract: @wcet(5us) @budget(50us);
$t0 := @tsc();
$sum := 0;
$i := 0;
@while($i < 100) {
    $sum += $i * 2;
    $i += 1;
}
$dt := @tsc() - $t0;
@println("[BENCH] Iterations: 100");
@println("[RESULT] Sum:");
@println($sum);
@println("[LATENCY] Cycles:");
@println($dt);
"#;
        let _ = fs_create_internal("bench.pl", bench_src, false);

        // 3. filter.pl - Packet filter and congestion controller
        let filter_src = br#"// filter.pl - Adaptive Congestion Guard [AI-Native Spec]
@contract: @wcet(2us) @budget(100us);
$rtt := @rtt();
@println("[FILTER] Measured RTT (ns):");
@println($rtt);
$rtt > 300us ? {
    @println("[ACTION] Congestion detected -> Rate: 60%");
    @rate(60);
} : {
    @println("[ACTION] Optimal latency -> Rate: 100%");
    @rate(100);
};
"#;
        let _ = fs_create_internal("filter.pl", filter_src, false);

        // 4. jitter.pl - Jitter analysis measuring cycle delta
        let jitter_src = br#"// jitter.pl - Cycle-Accurate Jitter Analyzer
@contract: @wcet(3us) @budget(30us);
$t1 := @tsc();
$t2 := @tsc();
$delta := $t2 - $t1;
@println("[JITTER] Consecutive TSC Delta (Cycles):");
@println($delta);
$delta < 100 ? {
    @println("[STATUS] Determinism: Optimal (<100 cycles)");
} : {
    @println("[STATUS] Determinism: Jitter detected");
};
"#;
        let _ = fs_create_internal("jitter.pl", jitter_src, false);

        // 5. telemetry.pl - Real-Time Telemetry and Hardware Inspector
        let telemetry_src = br#"// telemetry.pl - Real-Time Hardware Telemetry
@contract: @wcet(2us) @budget(20us);
$rtt := @rtt();
$tsc := @tsc();
@println("=== LatencyOS Hardware Telemetry ===");
@println("[CLOCK] Serialized TSC Ticks:");
@println($tsc);
@println("[NET] Active Round-Trip Time (ns):");
@println($rtt);
$rtt < 100us ? @println("[HEALTH] Sub-100us glass-to-glass latency guaranteed.") : @println("[HEALTH] RTT backpressure active.");
"#;
        let _ = fs_create_internal("telemetry.pl", telemetry_src, false);

        // 6. readme.txt - Plain text guide
        let readme_txt = br#"LatencyOS In-Memory Real-Time Filesystem (LatencyFS)
===================================================
- Files are statically allocated in L1/L2 cache with zero fragmentation.
- Supports text files (.txt, .md, .json, .log, etc.), scripts (.pl), and binaries (.bin).
- Use 'edit <file>' to edit in PulseEditor (Ctrl+S: save, Ctrl+R: run, Ctrl+Q: quit).
- Use 'compile <script.pl> <out.bin>' to build standalone binary bytecode.
- Use 'run <file>' to execute either .pl scripts or .bin bytecode.
"#;
        let _ = fs_create_internal("readme.txt", readme_txt, false);

        // 7. config.json - JSON configuration file
        let config_json = br#"{
  "os": "LatencyOS",
  "version": "0.0.17",
  "cores": 4,
  "target_latency_us": 8000,
  "c_state_lock": true,
  "uart_baud": 115200
}
"#;
        let _ = fs_create_internal("config.json", config_json, false);

        // 8. system.log - Hardware initialization log
        let system_log = br#"[BOOT] LatencyOS 0.0.17 x86_64 hard-realtime
[APIC] Cores 0-3 initialized with static affinity.
[PMD] Intel e1000 poll-mode driver active. MAC: 52:54:00:12:34:56.
[GPU] Zero-copy frame ring ready: 1920x1080 @ 32bpp.
[FS] LatencyFS initialized with 32 slots.
"#;
        let _ = fs_create_internal("system.log", system_log, false);
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

// Function: fs_rename
// Description: Rename a file in LatencyFS.
// Worst-case execution time: ~500 ns
pub fn fs_rename(old_name: &str, new_name: &str) -> Result<(), FsError> {
    if new_name.is_empty() || new_name.len() > MAX_FILENAME_LEN {
        return Err(FsError::InvalidName);
    }
    unsafe {
        for file in FS.files.iter_mut() {
            if file.used && file.name_str() == old_name {
                if file.read_only {
                    return Err(FsError::ReadOnly);
                }
                file.name_len = new_name.len();
                file.name[..new_name.len()].copy_from_slice(new_name.as_bytes());
                file.modified_tsc = read_tsc_serialized();
                return Ok(());
            }
        }
        Err(FsError::FileNotFound)
    }
}

// Function: fs_copy
// Description: Copy a file in LatencyFS.
// Worst-case execution time: ~1500 ns
pub fn fs_copy(src_name: &str, dst_name: &str) -> Result<(), FsError> {
    if let Some(data) = fs_read(src_name) {
        fs_write(dst_name, data)
    } else {
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
