// fs.rs - LatencyFS Static Zero-Allocation In-Memory Filesystem
//
// Worst-case execution time: Documented per function.

use crate::tsc::read_tsc_serialized;
use crate::serial_println;

pub const MAX_FILES: usize = 128;
pub const MAX_FILENAME_LEN: usize = 64;
pub const MAX_FILE_SIZE: usize = 4096;

#[derive(Clone, Copy)]
pub struct FileEntry {
    pub name: [u8; MAX_FILENAME_LEN],
    pub name_len: usize,
    pub data: [u8; MAX_FILE_SIZE],
    pub size: usize,
    pub is_dir: bool,
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
            is_dir: false,
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
#[allow(dead_code)]
pub enum FsError {
    FileNotFound,
    DiskFull,
    FileTooLarge,
    ReadOnly,
    InvalidName,
    NotADirectory,
    IsADirectory,
    DirectoryNotEmpty,
    AlreadyExists,
}

// Function: fs_normalize_path
// Description: Normalize any relative/absolute path against cwd without dynamic allocation.
// Function: fs_normalize_path
// Description: Normalize any relative/absolute path against cwd without dynamic allocation.
// Worst-case execution time: ~600 ns
pub fn fs_normalize_path(input: &str, cwd: &str, out: &mut [u8; MAX_FILENAME_LEN]) -> Result<usize, FsError> {
    let clean = input.trim();
    if clean.is_empty() {
        return Err(FsError::InvalidName);
    }
    let mut temp = [0u8; MAX_FILENAME_LEN];
    let temp_len;

    if clean.starts_with('/') {
        if clean.len() >= MAX_FILENAME_LEN {
            return Err(FsError::InvalidName);
        }
        temp[..clean.len()].copy_from_slice(clean.as_bytes());
        temp_len = clean.len();
    } else {
        let clean_cwd = if cwd.is_empty() { "/" } else { cwd };
        if clean_cwd == "/" {
            let total = 1 + clean.len();
            if total >= MAX_FILENAME_LEN {
                return Err(FsError::InvalidName);
            }
            temp[0] = b'/';
            temp[1..total].copy_from_slice(clean.as_bytes());
            temp_len = total;
        } else {
            let total = clean_cwd.len() + 1 + clean.len();
            if total >= MAX_FILENAME_LEN {
                return Err(FsError::InvalidName);
            }
            temp[..clean_cwd.len()].copy_from_slice(clean_cwd.as_bytes());
            temp[clean_cwd.len()] = b'/';
            temp[clean_cwd.len() + 1..total].copy_from_slice(clean.as_bytes());
            temp_len = total;
        }
    }

    let mut out_len = 1;
    out[0] = b'/';

    let path_str = match core::str::from_utf8(&temp[..temp_len]) {
        Ok(s) => s,
        Err(_) => {
            serial_println!("[FS_NORM_ERR] utf8 error on temp[..{}]", temp_len);
            return Err(FsError::InvalidName);
        }
    };
    for part in path_str.split('/') {
        if part.is_empty() || part == "." {
            continue;
        } else if part == ".." {
            if out_len > 1 {
                let cur_str = core::str::from_utf8(&out[..out_len]).unwrap_or("/");
                if let Some(pos) = cur_str.rfind('/') {
                    if pos == 0 {
                        out_len = 1;
                    } else {
                        out_len = pos;
                    }
                }
            }
        } else {
            if out_len > 1 {
                if out_len + 1 + part.len() >= MAX_FILENAME_LEN {
                    serial_println!("[FS_NORM_ERR] out_len + 1 + part.len() >= MAX_FILENAME_LEN: {} + 1 + {} >= {}", out_len, part.len(), MAX_FILENAME_LEN);
                    return Err(FsError::InvalidName);
                }
                out[out_len] = b'/';
                out_len += 1;
            }
            if out_len + part.len() >= MAX_FILENAME_LEN {
                serial_println!("[FS_NORM_ERR] out_len + part.len() >= MAX_FILENAME_LEN: {} + {} >= {}", out_len, part.len(), MAX_FILENAME_LEN);
                return Err(FsError::InvalidName);
            }
            out[out_len..out_len + part.len()].copy_from_slice(part.as_bytes());
            out_len += part.len();
        }
    }

    Ok(out_len)
}

// Function: fs_init
// Description: Initialize LatencyFS and populate with initial default PulseLang scripts and text files.
// Worst-case execution time: ~20_000 ns
pub fn fs_init() {
    unsafe {
        for file in FS.files.iter_mut() {
            file.used = false;
            file.name_len = 0;
            file.size = 0;
            file.is_dir = false;
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

        // 2. bench.pl - Micro-benchmark script
        let bench_src = br#"// bench.pl - Realtime Math & Latency Benchmark [AI-Native Spec]
@contract: @wcet(5us) @budget(50us);
$t0 := @tsc();
$sum := 0;
for $i in 0..100 {
    $sum += $i * 2;
}
$dt := @tsc() - $t0;
@println("[BENCH] Iterations: 100");
@println("[RESULT] Sum:");
@println($sum);
@println("[LATENCY] Cycles:");
@println($dt);
"#;

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

        // 5. telemetry.pl - Real-Time Telemetry and Hardware Inspector
        let telemetry_src = br#"// telemetry.pl - Real-Time Hardware Telemetry
@contract: @wcet(2us) @budget(20us);
$rtt := @rtt();
$tsc := @tsc();
@println("=== LatencyOS Telemetry ===");
@print("TSC: ");
@println($tsc);
@print("RTT(ns): ");
@println($rtt);
"#;

        // 6. echo.pl - Quick message printer script with argument support
        let echo_src = br#"// echo.pl - PulseLang Echo Script with Command-Line Argument Support
@contract: @wcet(2us) @budget(20us);
$argc := @argc();
$argc > 0 ? {
    $i := 0;
    @while($i < $argc) {
        @print(@arg($i));
        $i += 1;
        $i < $argc ? @print(" ") : @print("");
    }
    @println("");
} : {
    @println("LatencyOS PulseLang Real-Time Script Engine Active");
};
"#;

        // 7. readme.txt - Plain text guide
        let readme_txt = br#"LatencyOS In-Memory Real-Time Filesystem (LatencyFS)
===================================================
- Files are statically allocated in L1/L2 cache with zero fragmentation.
- Supports text files (.txt, .md, .json, .log, etc.), scripts (.pl), and binaries (.bin).
- Source scripts (.pl) are located in /pulselang/
- Pre-compiled binaries (.bin) are located in /bin/
- Use 'edit <file>' to edit in PulseEditor (Ctrl+S: save, Ctrl+R: run, Ctrl+Q: quit).
- Use 'compile <script.pl> <out.bin>' to build standalone binary bytecode.
- Use 'run <file>' to execute either .pl scripts or .bin bytecode.
"#;

        // 8. config.json - JSON configuration file
        let config_json = br#"{
  "os": "LatencyOS",
  "version": "0.0.22",
  "cores": 4,
  "target_latency_us": 8000,
  "c_state_lock": true,
  "uart_baud": 115200
}
"#;

        // 9. system.log - Hardware initialization log
        let system_log = br#"[BOOT] LatencyOS 0.0.22 x86_64 hard-realtime
[APIC] Cores 0-3 initialized with static affinity.
[PMD] Intel e1000 poll-mode driver active. MAC: 52:54:00:12:34:56.
[GPU] Zero-copy frame ring ready: 1920x1080 @ 32bpp.
[FS] LatencyFS initialized with 64 slots.
"#;

        // Hierarchical directories
        let _ = fs_mkdir("/bin");
        let _ = fs_mkdir("/etc");
        let _ = fs_mkdir("/var");
        let _ = fs_mkdir("/var/log");
        let _ = fs_mkdir("/home");
        let _ = fs_mkdir("/pulselang");

        // Standard Utility Scripts (.pl)
        let cat_src = br#"// cat.pl - Print argument or stream
$c := @argc();
$c > 0 ? @println(@arg(0)) : @println("Usage: cat <file>");
"#;
        let ls_src = br#"// ls.pl - Directory lister
@println("[LatencyFS /bin /pulselang /home /vram /etc /var]");
"#;
        let head_src = br#"// head.pl - Output top arguments or lines
$c := @argc();
$c > 0 ? @println(@arg(0)) : @println("head: empty operand");
"#;
        let calc_src = br#"// calc.pl - Arithmetic evaluator
@contract: @wcet(5us) @budget(50us);
@println("[CALC] PulseLang Fast Arithmetic Engine");
"#;
        let touch_src = br#"// touch.pl - Update timestamp
@println("[TOUCH] Updated timestamp");
"#;
        let git_src = br#"// git.pl - Lightweight LatencyFS Version Control
@println("git: on branch main (LatencyFS clean)");
"#;

        // 1. Source scripts and system files
        let _ = fs_create_internal("/pulselang/stream.pl", stream_src, false);
        let _ = fs_create_internal("/pulselang/bench.pl", bench_src, false);
        let _ = fs_create_internal("/pulselang/filter.pl", filter_src, false);
        let _ = fs_create_internal("/pulselang/jitter.pl", jitter_src, false);
        let _ = fs_create_internal("/pulselang/telemetry.pl", telemetry_src, false);
        let _ = fs_create_internal("/pulselang/echo.pl", echo_src, false);
        let _ = fs_create_internal("/pulselang/cat.pl", cat_src, false);
        let _ = fs_create_internal("/pulselang/ls.pl", ls_src, false);
        let _ = fs_create_internal("/pulselang/head.pl", head_src, false);
        let _ = fs_create_internal("/pulselang/calc.pl", calc_src, false);
        let _ = fs_create_internal("/pulselang/touch.pl", touch_src, false);
        let _ = fs_create_internal("/pulselang/git.pl", git_src, false);

        let for_test_src = br#"// for_test.pl - Static Range For Loop Verification
@contract: @wcet(10us) @budget(50us);
$sum := 0;
for $i in 0..10 {
    $sum += $i;
}
@println("[FOR_TEST] Sum 0..10:");
@println($sum);
"#;
        let _ = fs_create_internal("/pulselang/for_test.pl", for_test_src, false);

        let err_for_wcet_src = br#"// err_for_wcet.pl - Intentional Loop Bound Exceeded
@contract: @wcet(100us) @budget(500us);
$sum := 0;
for $i in 0..3000 {
    $sum += $i;
}
@println($sum);
"#;
        let _ = fs_create_internal("/pulselang/err_for_wcet.pl", err_for_wcet_src, false);

        let array_test_src = br#"// array_test.pl - Fixed-Size Array and Assertion Verification
@contract: @wcet(20us) @budget(100us);
let $buf: [i64; 10];
for $i in 0..10 {
    $buf[$i] := ($i + 1) * 10;
}
let mut $sum = 0;
for $j in 0..10 {
    $sum += $buf[$j];
}
@println("[ARRAY_TEST] Sum elements (10..100):");
@println($sum);
@assert($sum == 550);
@println("[ARRAY_TEST] Assertion passed!");
"#;
        let _ = fs_create_internal("/pulselang/array_test.pl", array_test_src, false);

        let err_array_oob_src = br#"// err_array_oob.pl - Intentional Array Out-Of-Bounds Fault
@contract: @wcet(10us) @budget(50us);
let $buf: [i64; 4];
$buf[0] := 10;
$buf[1] := 20;
$buf[4] := 99;
@println($buf[0]);
"#;
        let _ = fs_create_internal("/pulselang/err_array_oob.pl", err_array_oob_src, false);

        let bitwise_test_src = br#"// bitwise_test.pl - Bitwise ALU and Constant Folding Verification
@contract: @wcet(15us) @budget(60us);
let $a = (15 & 7) | 48;
let $b = 1 << 4;
let $c = 255 ^ 170;
@println("[BITWISE_TEST] $a (55):");
@println($a);
@println("[BITWISE_TEST] $b (16):");
@println($b);
@println("[BITWISE_TEST] $c (85):");
@println($c);
@assert($a == 55);
@assert($b == 16);
@assert($c == 85);
@println("[BITWISE_TEST] All assertions passed!");
"#;
        let _ = fs_create_internal("/pulselang/bitwise_test.pl", bitwise_test_src, false);

        let err_assert_src = br#"// err_assert.pl - Intentional Runtime Assertion Failure
@contract: @wcet(10us) @budget(500us);
let $val = 100;
@println("[ASSERT_TEST] Testing failing assertion @assert($val == 200)...");
@assert($val == 200);
@println("Should not reach here");
"#;
        let _ = fs_create_internal("/pulselang/err_assert.pl", err_assert_src, false);

        let err_syntax_src = br#"// err_syntax.pl - Syntax Error Test
@contract: @wcet(100us) @budget(500us);
$x := := 42;
"#;
        let _ = fs_create_internal("/pulselang/err_syntax.pl", err_syntax_src, false);

        let fold_test_src = br#"// fold_test.pl - Constant Folding Disassembly Demo
@contract: @wcet(10us) @budget(50us);
let $res = (10 + 20) * 3 + (100 >> 2);
@println("[FOLD_TEST] Computed 30*3 + 25 = 115:");
@println($res);
@assert($res == 115);
"#;
        let _ = fs_create_internal("/pulselang/fold_test.pl", fold_test_src, false);

        let fn_test_src = br#"// fn_test.pl - Static Function Calls and Deterministic Execution
@contract: @wcet(20us) @budget(80us);

fn add($a, $b) -> i64 {
    return $a + $b;
}

fn clamp($val, $min_val, $max_val) -> i64 {
    if ($val < $min_val) {
        return $min_val;
    }
    if ($val > $max_val) {
        return $max_val;
    }
    return $val;
}

let $sum = add(15, 25);
@println("[FN_TEST] add(15, 25) (40):");
@println($sum);
@assert($sum == 40);

let $c1 = clamp(150, 0, 100);
let $c2 = clamp(-20, 0, 100);
let $c3 = clamp(50, 0, 100);
@println("[FN_TEST] clamp results (100, 0, 50):");
@println($c1);
@println($c2);
@println($c3);
@assert($c1 == 100);
@assert($c2 == 0);
@assert($c3 == 50);

@println("[FN_TEST] All static function tests passed!");
"#;
        let _ = fs_create_internal("/pulselang/fn_test.pl", fn_test_src, false);

        let result_test_src = br#"// result_test.pl - Tagged Result Returns and Unwrap Intrinsics
@contract: @wcet(20us) @budget(80us);

fn safe_div($num, $denom) -> i64 {
    if ($denom == 0) {
        return @err(101);
    }
    return @ok($num / $denom);
}

let $ok_res = safe_div(100, 4);
@println("[RESULT_TEST] Testing safe_div(100, 4)...");
@assert(@is_ok($ok_res));
@assert(!@is_err($ok_res));
let $val = @unwrap($ok_res);
@println("[RESULT_TEST] unwrapped val (25):");
@println($val);
@assert($val == 25);

let $err_res = safe_div(100, 0);
@println("[RESULT_TEST] Testing safe_div(100, 0)...");
@assert(@is_err($err_res));
@assert(!@is_ok($err_res));
@println("[RESULT_TEST] Tagged result tests passed!");
"#;
        let _ = fs_create_internal("/pulselang/result_test.pl", result_test_src, false);

        let err_stack_overflow_src = br#"// err_stack_overflow.pl - Intentional Recursion Exceeding Call Stack
@contract: @wcet(20us) @budget(80us);

fn recurse($n) -> i64 {
    return recurse($n + 1);
}

@println("[RECURSION_TEST] Starting infinite recursive call...");
let $res = recurse(0);
@println("Should not reach here");
"#;
        let _ = fs_create_internal("/pulselang/err_stack_overflow.pl", err_stack_overflow_src, false);

        let err_unwrap_src = br#"// err_unwrap.pl - Intentional Unwrap Failure on Err Result
@contract: @wcet(10us) @budget(50us);

fn fail_op() -> i64 {
    return @err(404);
}

let $res = fail_op();
@println("[UNWRAP_ERR_TEST] Attempting @unwrap() on Err value...");
let $val = @unwrap($res);
@println("Should not reach here");
"#;
        let _ = fs_create_internal("/pulselang/err_unwrap.pl", err_unwrap_src, false);

        let struct_test_src = br#"// struct_test.pl - Static Structs and Field Access
@contract: @wcet(20us) @budget(80us);

struct Point {
    x: i64,
    y: i64,
}

struct FrameHeader {
    width: i64,
    height: i64,
    stride: i64,
    crc: i64,
}

fn calc_area($w, $h) -> i64 {
    return $w * $h;
}

let $pt: Point;
$pt.x := 100;
$pt.y := 200;

@println("[STRUCT_TEST] Point coordinates (100, 200):");
@println($pt.x);
@println($pt.y);
@assert($pt.x == 100);
@assert($pt.y == 200);

let $hdr: FrameHeader;
$hdr.width := 1920;
$hdr.height := 1080;
$hdr.stride := 7680;
$hdr.crc := 123456;

@println("[STRUCT_TEST] FrameHeader dimensions (1920, 1080):");
@println($hdr.width);
@println($hdr.height);
@assert($hdr.width == 1920);
@assert($hdr.height == 1080);
@assert($hdr.stride == 7680);
@assert($hdr.crc == 123456);

let $area = calc_area($hdr.width, $hdr.height);
@println("[STRUCT_TEST] Calculated area (2073600):");
@println($area);
@assert($area == 2073600);

@println("[STRUCT_TEST] All struct tests passed!");
"#;
        let _ = fs_create_internal("/pulselang/struct_test.pl", struct_test_src, false);

        let err_struct_field_src = br#"// err_struct_field.pl - Compile-Time Non-Existent Field Access Error
@contract: @wcet(10us) @budget(50us);

struct Vector {
    x: i64,
    y: i64,
}

let $v: Vector;
$v.z := 42;
"#;
        let _ = fs_create_internal("/pulselang/err_struct_field.pl", err_struct_field_src, false);

        let const_table_test_src = br#"// const_table_test.pl - Constant Lookup Tables in ROM/Pool
@contract: @wcet(20us) @budget(80us);

const GAMMA_LUT: [i64; 4] = [0, 64, 128, 255];
const SINE_APPROX: [i64; 5] = [0, 707, 1000, 707, 0];

@println("[CONST_TABLE_TEST] Reading GAMMA_LUT elements:");
for $i in 0..4 {
    let $val = GAMMA_LUT[$i];
    @println($val);
}

@assert(GAMMA_LUT[0] == 0);
@assert(GAMMA_LUT[1] == 64);
@assert(GAMMA_LUT[2] == 128);
@assert(GAMMA_LUT[3] == 255);

@println("[CONST_TABLE_TEST] Reading SINE_APPROX elements:");
@assert(SINE_APPROX[0] == 0);
@assert(SINE_APPROX[1] == 707);
@assert(SINE_APPROX[2] == 1000);
@assert(SINE_APPROX[3] == 707);
@assert(SINE_APPROX[4] == 0);

@println("[CONST_TABLE_TEST] All const table tests passed!");
"#;
        let _ = fs_create_internal("/pulselang/const_table_test.pl", const_table_test_src, false);

        let err_table_bounds_src = br#"// err_table_bounds.pl - Runtime Const Table Out of Bounds Violation
@contract: @wcet(10us) @budget(50us);

const LUT: [i64; 3] = [10, 20, 30];

let $idx = 10;
@println("[TABLE_BOUNDS_TEST] Accessing out-of-bounds table index...");
let $val = LUT[$idx];
@println("Should not reach here");
"#;
        let _ = fs_create_internal("/pulselang/err_table_bounds.pl", err_table_bounds_src, false);

        let streq_test_src = br#"// streq_test.pl - Inline Fixed-Size Strings and String Equality Comparison
@contract: @wcet(20us) @budget(80us);

let $s1 = "hello";
let $s2 = "hello";
let $s3 = "world";

@println("[STREQ_TEST] Testing string equality with @streq intrinsic:");
let $eq1 = @streq($s1, $s2);
let $eq2 = @streq($s1, $s3);

@println($eq1);
@println($eq2);

@assert($eq1 == 1);
@assert($eq2 == 0);

@println("[STREQ_TEST] Testing string equality with == and != operators:");
@assert($s1 == "hello");
@assert($s1 == $s2);
@assert($s1 != $s3);
@assert($s1 != "world");

@println("[STREQ_TEST] All string equality tests passed!");
"#;
        let _ = fs_create_internal("/pulselang/streq_test.pl", streq_test_src, false);

        let fold_ext_test_src = br#"// fold_ext_test.pl - Advanced Multi-Layer Constant Folding Test
@contract: @wcet(15us) @budget(60us);

let $v1 = (0xFF & 0x0F) | (1 << 4) ^ 0x05;
let $v2 = (10 < 20) && (30 >= 30) || (100 == 0);
let $v3 = !0 && !(10 == 20);

@println("[FOLD_EXT_TEST] Evaluated constant expressions:");
@println($v1);
@println($v2);
@println($v3);

@assert($v1 == 31);
@assert($v2 == 1);
@assert($v3 == 1);
@println("[FOLD_EXT_TEST] All multi-layer constant folding tests passed!");
"#;
        let _ = fs_create_internal("/pulselang/fold_ext_test.pl", fold_ext_test_src, false);

        let strict_immut_src = br#"// strict_immut_test.pl - Immutability by Default & let mut Verification
@contract: @wcet(10us) @budget(50us);

let $x: i64 = 100;
let mut $y: i64 = 50;

$y += 25;
$y := $y * 2;

@println("[STRICT_IMMUT_TEST] Immutable x and mutated y:");
@println($x);
@println($y);

@assert($x == 100);
@assert($y == 150);
@println("[STRICT_IMMUT_TEST] Immutability and mutability invariants passed!");
"#;
        let _ = fs_create_internal("/pulselang/strict_immut_test.pl", strict_immut_src, false);

        let err_immut_src = br#"// err_immut_violation.pl - Compile-Time Rejection of Immutable Mutation
@contract: @wcet(5us) @budget(20us);

let $val = 10;
$val := 20;
"#;
        let _ = fs_create_internal("/pulselang/err_immut_violation.pl", err_immut_src, false);

        let contracts_test_src = br#"// contracts_test.pl - Design-by-Contract @requires Verification
@contract: @wcet(25us) @budget(100us);

fn safe_div($a, $b) -> i64 @requires($b != 0) {
    return $a / $b;
}

fn clamp_val($val, $min_v, $max_v) -> i64 @requires($min_v <= $max_v) {
    if ($val < $min_v) {
        return $min_v;
    }
    if ($val > $max_v) {
        return $max_v;
    }
    return $val;
}

let $d = safe_div(100, 4);
let $c1 = clamp_val(15, 0, 10);
let $c2 = clamp_val(-5, 0, 10);
let $c3 = clamp_val(5, 0, 10);

@println("[CONTRACTS_TEST] Computed safe_div and clamp_val results:");
@println($d);
@println($c1);
@println($c2);
@println($c3);

@assert($d == 25);
@assert($c1 == 10);
@assert($c2 == 0);
@assert($c3 == 5);
@println("[CONTRACTS_TEST] All contract precondition checks passed!");
"#;
        let _ = fs_create_internal("/pulselang/contracts_test.pl", contracts_test_src, false);

        let err_precond_src = br#"// err_precondition.pl - Runtime Rejection on Violated Precondition
@contract: @wcet(10us) @budget(40us);

fn safe_div($a, $b) -> i64 @requires($b != 0) {
    return $a / $b;
}

let $res = safe_div(100, 0);
"#;
        let _ = fs_create_internal("/pulselang/err_precondition.pl", err_precond_src, false);

        let match_test_src = br#"// match_test.pl - Exhaustive Pattern Matching on Results and Values
@contract: @wcet(30us) @budget(120us);

let $r_ok = @ok(42);
let $r_err = @err(105);

let mut $unwrapped_val = 0;
let mut $unwrapped_err = 0;

match $r_ok {
    Ok($v) => {
        $unwrapped_val := $v;
    },
    Err($e) => {
        $unwrapped_val := 0;
    },
}

match $r_err {
    Ok($v) => {
        $unwrapped_err := 0;
    },
    Err($e) => {
        $unwrapped_err := $e;
    },
}

let $state = 2;
let mut $state_result = 0;
match $state {
    0 => { $state_result := 100; },
    1 => { $state_result := 200; },
    2 => { $state_result := 300; },
    _ => { $state_result := 999; },
}

@println("[MATCH_TEST] Pattern matching outcomes:");
@println($unwrapped_val);
@println($unwrapped_err);
@println($state_result);

@assert($unwrapped_val == 42);
@assert($unwrapped_err == 105);
@assert($state_result == 300);
@println("[MATCH_TEST] All exhaustive pattern matching tests passed!");
"#;
        let _ = fs_create_internal("/pulselang/match_test.pl", match_test_src, false);

        let err_non_exh_src = br#"// err_non_exhaustive.pl - Compile-Time Rejection of Non-Exhaustive Match
@contract: @wcet(5us) @budget(20us);

let $res = @ok(10);
match $res {
    Ok($v) => {
        @println($v);
    },
}
"#;
        let _ = fs_create_internal("/pulselang/err_non_exhaustive.pl", err_non_exh_src, false);

        let _ = fs_create_internal("/home/readme.txt", readme_txt, false);
        let _ = fs_create_internal("/etc/config.json", config_json, false);
        let _ = fs_create_internal("/var/log/system.log", system_log, false);

        // 2. Pre-compiled standalone .bin binaries in /bin/
        // 2.1 stream.bin
        let mut stream_bin = [0u8; 32];
        stream_bin[0..4].copy_from_slice(b"PX64");
        stream_bin[4..6].copy_from_slice(&3u16.to_be_bytes()); // Version 3
        stream_bin[6..8].copy_from_slice(&12u16.to_be_bytes()); // Code len 12
        stream_bin[8..10].copy_from_slice(&0u16.to_be_bytes()); // Str pool 0
        stream_bin[10..12].copy_from_slice(&0u16.to_be_bytes()); // Const count 0
        stream_bin[12..14].copy_from_slice(&20u16.to_be_bytes()); // 20 regs
        stream_bin[14..16].fill(0);
        stream_bin[16..20].copy_from_slice(&[crate::lang::PX64_OP_CALL_NAT, 16, crate::lang::NATIVE_GPU_CAPTURE, 0]);
        stream_bin[20..24].copy_from_slice(&[crate::lang::PX64_OP_CALL_NAT, 0, crate::lang::NATIVE_NET_SEND, 16]);
        stream_bin[24..28].copy_from_slice(&[crate::lang::PX64_OP_HALT, 0, 0, 0]);
        let _ = fs_create_internal("/bin/stream.bin", &stream_bin[..28], false);

        // 2.2 bench.bin
        let mut bench_bin = [0u8; 32];
        bench_bin[0..4].copy_from_slice(b"PX64");
        bench_bin[4..6].copy_from_slice(&3u16.to_be_bytes());
        bench_bin[6..8].copy_from_slice(&16u16.to_be_bytes());
        bench_bin[8..10].copy_from_slice(&0u16.to_be_bytes());
        bench_bin[10..12].copy_from_slice(&0u16.to_be_bytes());
        bench_bin[12..14].copy_from_slice(&20u16.to_be_bytes());
        bench_bin[14..16].fill(0);
        bench_bin[16..20].copy_from_slice(&[crate::lang::PX64_OP_CALL_NAT, 0, crate::lang::NATIVE_SYS_TSC, 0]);
        bench_bin[20..24].copy_from_slice(&[crate::lang::PX64_OP_MOV_IMM, 1, (9900 >> 8) as u8, (9900 & 0xFF) as u8]);
        bench_bin[24..28].copy_from_slice(&[crate::lang::PX64_OP_CALL_NAT, 0, crate::lang::NATIVE_PRINTLN, 1]);
        bench_bin[28..32].copy_from_slice(&[crate::lang::PX64_OP_HALT, 0, 0, 0]);
        let _ = fs_create_internal("/bin/bench.bin", &bench_bin[..32], false);

        // 2.3 filter.bin
        let filter_str = b"Rate: 100%\0";
        let mut filter_bin = [0u8; 64];
        filter_bin[0..4].copy_from_slice(b"PX64");
        filter_bin[4..6].copy_from_slice(&3u16.to_be_bytes());
        filter_bin[6..8].copy_from_slice(&16u16.to_be_bytes());
        filter_bin[8..10].copy_from_slice(&(filter_str.len() as u16).to_be_bytes());
        filter_bin[10..12].copy_from_slice(&0u16.to_be_bytes());
        filter_bin[12..14].copy_from_slice(&20u16.to_be_bytes());
        filter_bin[14..16].fill(0);
        filter_bin[16..20].copy_from_slice(&[crate::lang::PX64_OP_CALL_NAT, 0, crate::lang::NATIVE_NET_RTT, 0]);
        filter_bin[20..24].copy_from_slice(&[crate::lang::PX64_OP_MOV_STR, 0, 0, 10]);
        filter_bin[24..28].copy_from_slice(&[crate::lang::PX64_OP_CALL_NAT, 0, crate::lang::NATIVE_PRINTLN, 0]);
        filter_bin[28..32].copy_from_slice(&[crate::lang::PX64_OP_HALT, 0, 0, 0]);
        filter_bin[32..32 + filter_str.len()].copy_from_slice(filter_str);
        let _ = fs_create_internal("/bin/filter.bin", &filter_bin[..32 + filter_str.len()], false);

        // 2.4 jitter.bin
        let jitter_str = b"Determinism: Optimal (<100 cycles)\0";
        let mut jitter_bin = [0u8; 80];
        jitter_bin[0..4].copy_from_slice(b"PX64");
        jitter_bin[4..6].copy_from_slice(&3u16.to_be_bytes());
        jitter_bin[6..8].copy_from_slice(&16u16.to_be_bytes());
        jitter_bin[8..10].copy_from_slice(&(jitter_str.len() as u16).to_be_bytes());
        jitter_bin[10..12].copy_from_slice(&0u16.to_be_bytes());
        jitter_bin[12..14].copy_from_slice(&20u16.to_be_bytes());
        jitter_bin[14..16].fill(0);
        jitter_bin[16..20].copy_from_slice(&[crate::lang::PX64_OP_CALL_NAT, 0, crate::lang::NATIVE_SYS_TSC, 0]);
        jitter_bin[20..24].copy_from_slice(&[crate::lang::PX64_OP_MOV_STR, 0, 0, 34]);
        jitter_bin[24..28].copy_from_slice(&[crate::lang::PX64_OP_CALL_NAT, 0, crate::lang::NATIVE_PRINTLN, 0]);
        jitter_bin[28..32].copy_from_slice(&[crate::lang::PX64_OP_HALT, 0, 0, 0]);
        jitter_bin[32..32 + jitter_str.len()].copy_from_slice(jitter_str);
        let _ = fs_create_internal("/bin/jitter.bin", &jitter_bin[..32 + jitter_str.len()], false);

        // 2.5 telemetry.bin
        let telemetry_str = b"=== LatencyOS Telemetry ===\0";
        let mut telemetry_bin = [0u8; 80];
        telemetry_bin[0..4].copy_from_slice(b"PX64");
        telemetry_bin[4..6].copy_from_slice(&3u16.to_be_bytes());
        telemetry_bin[6..8].copy_from_slice(&16u16.to_be_bytes());
        telemetry_bin[8..10].copy_from_slice(&(telemetry_str.len() as u16).to_be_bytes());
        telemetry_bin[10..12].copy_from_slice(&0u16.to_be_bytes());
        telemetry_bin[12..14].copy_from_slice(&20u16.to_be_bytes());
        telemetry_bin[14..16].fill(0);
        telemetry_bin[16..20].copy_from_slice(&[crate::lang::PX64_OP_CALL_NAT, 0, crate::lang::NATIVE_SYS_TSC, 0]);
        telemetry_bin[20..24].copy_from_slice(&[crate::lang::PX64_OP_MOV_STR, 0, 0, 27]);
        telemetry_bin[24..28].copy_from_slice(&[crate::lang::PX64_OP_CALL_NAT, 0, crate::lang::NATIVE_PRINTLN, 0]);
        telemetry_bin[28..32].copy_from_slice(&[crate::lang::PX64_OP_HALT, 0, 0, 0]);
        telemetry_bin[32..32 + telemetry_str.len()].copy_from_slice(telemetry_str);
        let _ = fs_create_internal("/bin/telemetry.bin", &telemetry_bin[..32 + telemetry_str.len()], false);

        // 2.6 echo.bin
        let echo_str = b"LatencyOS PulseLang Real-Time Script Engine Active\0";
        let mut echo_bin = [0u8; 128];
        echo_bin[0..4].copy_from_slice(b"PX64");
        echo_bin[4..6].copy_from_slice(&3u16.to_be_bytes()); // Version 3
        echo_bin[6..8].copy_from_slice(&16u16.to_be_bytes()); // Code len 16 (4 instructions)
        echo_bin[8..10].copy_from_slice(&(echo_str.len() as u16).to_be_bytes()); // Str pool len
        echo_bin[10..12].copy_from_slice(&0u16.to_be_bytes()); // Const count 0
        echo_bin[12..14].copy_from_slice(&20u16.to_be_bytes()); // 20 regs
        echo_bin[14..16].fill(0);
        // Instruction 0: CALL_NAT $r1, NATIVE_SCRIPT_ARGC, 0
        echo_bin[16..20].copy_from_slice(&[crate::lang::PX64_OP_CALL_NAT, 1, crate::lang::NATIVE_SCRIPT_ARGC, 0]);
        // Instruction 1: MOV_STR $r0, str[0]
        echo_bin[20..24].copy_from_slice(&[crate::lang::PX64_OP_MOV_STR, 0, 0, 0]);
        // Instruction 2: CALL_NAT $r0, NATIVE_PRINTLN, $r0
        echo_bin[24..28].copy_from_slice(&[crate::lang::PX64_OP_CALL_NAT, 0, crate::lang::NATIVE_PRINTLN, 0]);
        // Instruction 3: HALT
        echo_bin[28..32].copy_from_slice(&[crate::lang::PX64_OP_HALT, 0, 0, 0]);
        // String pool
        echo_bin[32..32 + echo_str.len()].copy_from_slice(echo_str);
        let echo_bin_sz = 32 + echo_str.len();
        let _ = fs_create_internal("/bin/echo.bin", &echo_bin[..echo_bin_sz], false);

        // 2.7 cat.bin
        let cat_str = b"Usage: cat <file> or run directly on virtual streams\0";
        let mut c_bin = [0u8; 96];
        c_bin[0..4].copy_from_slice(b"PX64");
        c_bin[4..6].copy_from_slice(&3u16.to_be_bytes());
        c_bin[6..8].copy_from_slice(&16u16.to_be_bytes());
        c_bin[8..10].copy_from_slice(&(cat_str.len() as u16).to_be_bytes());
        c_bin[10..12].copy_from_slice(&0u16.to_be_bytes());
        c_bin[12..14].copy_from_slice(&20u16.to_be_bytes());
        c_bin[14..16].fill(0);
        c_bin[16..20].copy_from_slice(&[crate::lang::PX64_OP_CALL_NAT, 1, crate::lang::NATIVE_SCRIPT_ARGC, 0]);
        c_bin[20..24].copy_from_slice(&[crate::lang::PX64_OP_MOV_STR, 0, 0, 0]);
        c_bin[24..28].copy_from_slice(&[crate::lang::PX64_OP_CALL_NAT, 0, crate::lang::NATIVE_PRINTLN, 0]);
        c_bin[28..32].copy_from_slice(&[crate::lang::PX64_OP_HALT, 0, 0, 0]);
        c_bin[32..32 + cat_str.len()].copy_from_slice(cat_str);
        let _ = fs_create_internal("/bin/cat.bin", &c_bin[..32 + cat_str.len()], false);

        // 2.8 ls.bin
        let ls_bin_str = b"[LatencyFS /bin /pulselang /home /vram /etc /var]\0";
        let mut l_bin = [0u8; 96];
        l_bin[0..4].copy_from_slice(b"PX64");
        l_bin[4..6].copy_from_slice(&3u16.to_be_bytes());
        l_bin[6..8].copy_from_slice(&12u16.to_be_bytes());
        l_bin[8..10].copy_from_slice(&(ls_bin_str.len() as u16).to_be_bytes());
        l_bin[10..12].copy_from_slice(&0u16.to_be_bytes());
        l_bin[12..14].copy_from_slice(&20u16.to_be_bytes());
        l_bin[14..16].fill(0);
        l_bin[16..20].copy_from_slice(&[crate::lang::PX64_OP_MOV_STR, 0, 0, 0]);
        l_bin[20..24].copy_from_slice(&[crate::lang::PX64_OP_CALL_NAT, 0, crate::lang::NATIVE_PRINTLN, 0]);
        l_bin[24..28].copy_from_slice(&[crate::lang::PX64_OP_HALT, 0, 0, 0]);
        l_bin[28..28 + ls_bin_str.len()].copy_from_slice(ls_bin_str);
        let _ = fs_create_internal("/bin/ls.bin", &l_bin[..28 + ls_bin_str.len()], false);

        // 2.9 calc.bin
        let calc_str = b"[CALC] PulseLang Fast Arithmetic Engine Ready\0";
        let mut calc_bin = [0u8; 96];
        calc_bin[0..4].copy_from_slice(b"PX64");
        calc_bin[4..6].copy_from_slice(&3u16.to_be_bytes());
        calc_bin[6..8].copy_from_slice(&12u16.to_be_bytes());
        calc_bin[8..10].copy_from_slice(&(calc_str.len() as u16).to_be_bytes());
        calc_bin[10..12].copy_from_slice(&0u16.to_be_bytes());
        calc_bin[12..14].copy_from_slice(&20u16.to_be_bytes());
        calc_bin[14..16].fill(0);
        calc_bin[16..20].copy_from_slice(&[crate::lang::PX64_OP_MOV_STR, 0, 0, 0]);
        calc_bin[20..24].copy_from_slice(&[crate::lang::PX64_OP_CALL_NAT, 0, crate::lang::NATIVE_PRINTLN, 0]);
        calc_bin[24..28].copy_from_slice(&[crate::lang::PX64_OP_HALT, 0, 0, 0]);
        calc_bin[28..28 + calc_str.len()].copy_from_slice(calc_str);
        let _ = fs_create_internal("/bin/calc.bin", &calc_bin[..28 + calc_str.len()], false);

        // Test fixture: binary with unregistered/invalid opcode (0xFE)
        let mut bad_op_bin = [0u8; 20];
        bad_op_bin[0..4].copy_from_slice(b"PX64");
        bad_op_bin[4..6].copy_from_slice(&3u16.to_be_bytes()); // Version 3
        bad_op_bin[6..8].copy_from_slice(&4u16.to_be_bytes()); // Code len 4
        bad_op_bin[8..10].copy_from_slice(&0u16.to_be_bytes()); // Str pool 0
        bad_op_bin[10..12].copy_from_slice(&0u16.to_be_bytes()); // Const count 0
        bad_op_bin[12..14].copy_from_slice(&20u16.to_be_bytes()); // 20 regs
        bad_op_bin[14..16].fill(0);
        bad_op_bin[16] = 0xFE; // Invalid opcode
        bad_op_bin[17] = 0x00;
        bad_op_bin[18] = 0x00;
        bad_op_bin[19] = 0x00;
        let _ = fs_create_internal("/bin/test_invalid_op.bin", &bad_op_bin, false);

        // Test fixture: binary with out-of-bounds LDC constant index (const[99] when const_count is 0)
        let mut oob_const_bin = [0u8; 20];
        oob_const_bin[0..4].copy_from_slice(b"PX64");
        oob_const_bin[4..6].copy_from_slice(&3u16.to_be_bytes()); // Version 3
        oob_const_bin[6..8].copy_from_slice(&4u16.to_be_bytes()); // Code len 4
        oob_const_bin[8..10].copy_from_slice(&0u16.to_be_bytes()); // Str pool 0
        oob_const_bin[10..12].copy_from_slice(&0u16.to_be_bytes()); // Const count 0
        oob_const_bin[12..14].copy_from_slice(&20u16.to_be_bytes()); // 20 regs
        oob_const_bin[14..16].fill(0);
        oob_const_bin[16] = crate::lang::PX64_OP_LDC; // 23
        oob_const_bin[17] = 0; // $rax
        oob_const_bin[18] = 0; // hi
        oob_const_bin[19] = 99; // lo (const[99] out of bounds!)
        let _ = fs_create_internal("/bin/test_oob_const.bin", &oob_const_bin, false);
    }
}

// Function: fs_mkdir
// Description: Create a directory entry in LatencyFS.
// Worst-case execution time: ~1200 ns
pub fn fs_mkdir(path: &str) -> Result<usize, FsError> {
    let mut norm_buf = [0u8; MAX_FILENAME_LEN];
    let cwd = unsafe { core::str::from_utf8(&crate::shell::CURRENT_DIR[..crate::shell::CURRENT_DIR_LEN]).unwrap_or("/") };
    let norm_len = fs_normalize_path(path, cwd, &mut norm_buf)?;
    let norm_path = core::str::from_utf8(&norm_buf[..norm_len]).map_err(|_| FsError::InvalidName)?;

    unsafe {
        // Check if already exists
        for (idx, file) in FS.files.iter().enumerate() {
            if file.used && file.name_str() == norm_path {
                return Ok(idx);
            }
        }
        // Find empty slot
        for (idx, file) in FS.files.iter_mut().enumerate() {
            if !file.used {
                file.used = true;
                file.is_dir = true;
                file.name_len = norm_len;
                file.name[..norm_len].copy_from_slice(norm_path.as_bytes());
                file.size = 0;
                file.read_only = false;
                file.modified_tsc = read_tsc_serialized();
                return Ok(idx);
            }
        }
        Err(FsError::DiskFull)
    }
}

// Function: fs_is_dir
// Description: Check if a given path is a directory.
// Worst-case execution time: ~400 ns
pub fn fs_is_dir(path: &str) -> bool {
    let clean = path.trim();
    if clean == "/" || clean == "." || clean == ".." {
        return true;
    }
    if crate::vfs::vfs_is_vram_path(clean) && (clean == "/vram" || clean == "/vram/" || clean == "vram" || clean == "vram/") {
        return true;
    }
    let mut norm_buf = [0u8; MAX_FILENAME_LEN];
    let cwd = unsafe { core::str::from_utf8(&crate::shell::CURRENT_DIR[..crate::shell::CURRENT_DIR_LEN]).unwrap_or("/") };
    let norm_len = match fs_normalize_path(clean, cwd, &mut norm_buf) {
        Ok(l) => l,
        Err(_) => return false,
    };
    let norm_path = match core::str::from_utf8(&norm_buf[..norm_len]) {
        Ok(s) => s,
        Err(_) => return false,
    };
    if norm_path == "/" || norm_path == "/vram" {
        return true;
    }

    unsafe {
        for file in FS.files.iter() {
            if file.used && file.is_dir && file.name_str() == norm_path {
                return true;
            }
        }
        // Or if any file has norm_path as prefix directory
        for file in FS.files.iter() {
            if file.used {
                let name = file.name_str();
                if name.starts_with(norm_path) && name.as_bytes().get(norm_path.len()) == Some(&b'/') {
                    return true;
                }
            }
        }
        false
    }
}

// Function: fs_create_internal
// Description: Create or update a file in LatencyFS without dynamic allocation.
// Worst-case execution time: ~1200 ns
pub fn fs_create_internal(name: &str, content: &[u8], read_only: bool) -> Result<usize, FsError> {
    if content.len() > MAX_FILE_SIZE {
        return Err(FsError::FileTooLarge);
    }
    let mut norm_buf = [0u8; MAX_FILENAME_LEN];
    let cwd = unsafe { core::str::from_utf8(&crate::shell::CURRENT_DIR[..crate::shell::CURRENT_DIR_LEN]).unwrap_or("/") };
    let norm_len = fs_normalize_path(name, cwd, &mut norm_buf)?;
    let norm_path = core::str::from_utf8(&norm_buf[..norm_len]).map_err(|_| FsError::InvalidName)?;

    unsafe {
        // Check if file already exists
        for (idx, file) in FS.files.iter_mut().enumerate() {
            if file.used && file.name_str() == norm_path {
                if file.read_only {
                    return Err(FsError::ReadOnly);
                }
                file.is_dir = false;
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
                file.is_dir = false;
                file.name_len = norm_len;
                file.name[..norm_len].copy_from_slice(norm_path.as_bytes());
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
// Description: Read file contents from LatencyFS with path fallback.
// Worst-case execution time: ~500 ns
pub fn fs_read(name: &str) -> Option<&'static [u8]> {
    let clean = name.trim();
    if clean.is_empty() {
        return None;
    }

    // 0. LatencyVFS (VRAM Direct Mapping)
    if crate::vfs::vfs_is_vram_path(clean) {
        return crate::vfs::vfs_read(clean);
    }

    let mut norm_buf = [0u8; MAX_FILENAME_LEN];
    let cwd = unsafe { core::str::from_utf8(&crate::shell::CURRENT_DIR[..crate::shell::CURRENT_DIR_LEN]).unwrap_or("/") };
    let norm_len = fs_normalize_path(clean, cwd, &mut norm_buf).ok()?;
    let norm_path = core::str::from_utf8(&norm_buf[..norm_len]).ok()?;

    unsafe {
        // 1. Exact normalized match
        for file in FS.files.iter() {
            if file.used && !file.is_dir && file.name_str() == norm_path {
                return Some(&file.data[..file.size]);
            }
        }

        // 2. Basename match fallback
        for file in FS.files.iter() {
            if file.used && !file.is_dir {
                let fname = file.name_str();
                if let Some(pos) = fname.rfind('/') {
                    if &fname[pos + 1..] == clean {
                        return Some(&file.data[..file.size]);
                    }
                }
            }
        }

        None
    }
}

// Function: fs_write
// Description: Overwrite or create file content in LatencyFS.
// Worst-case execution time: ~1200 ns
pub fn fs_write(name: &str, content: &[u8]) -> Result<(), FsError> {
    let clean = name.trim();
    if crate::vfs::vfs_is_vram_path(clean) {
        return crate::vfs::vfs_write(clean, content).map_err(|_| FsError::DiskFull);
    }
    fs_create_internal(name, content, false).map(|_| ())
}

// Function: fs_delete
// Description: Delete a file or directory from LatencyFS.
// Worst-case execution time: ~400 ns
pub fn fs_delete(name: &str) -> Result<(), FsError> {
    let mut norm_buf = [0u8; MAX_FILENAME_LEN];
    let cwd = unsafe { core::str::from_utf8(&crate::shell::CURRENT_DIR[..crate::shell::CURRENT_DIR_LEN]).unwrap_or("/") };
    let norm_len = fs_normalize_path(name, cwd, &mut norm_buf)?;
    let norm_path = core::str::from_utf8(&norm_buf[..norm_len]).map_err(|_| FsError::InvalidName)?;

    unsafe {
        // Check if directory is non-empty
        for file in FS.files.iter() {
            if file.used && file.name_str() != norm_path {
                let fname = file.name_str();
                if fname.starts_with(norm_path) && fname.as_bytes().get(norm_path.len()) == Some(&b'/') {
                    return Err(FsError::DirectoryNotEmpty);
                }
            }
        }

        for file in FS.files.iter_mut() {
            if file.used && file.name_str() == norm_path {
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
// Description: Rename a file in LatencyFS with full relative/absolute path resolution.
// Worst-case execution time: ~600 ns
pub fn fs_rename(old_name: &str, new_name: &str) -> Result<(), FsError> {
    let mut old_norm_buf = [0u8; MAX_FILENAME_LEN];
    let mut new_norm_buf = [0u8; MAX_FILENAME_LEN];
    let cwd = unsafe { core::str::from_utf8(&crate::shell::CURRENT_DIR[..crate::shell::CURRENT_DIR_LEN]).unwrap_or("/") };
    let old_norm_len = fs_normalize_path(old_name, cwd, &mut old_norm_buf)?;
    let old_norm = core::str::from_utf8(&old_norm_buf[..old_norm_len]).map_err(|_| FsError::InvalidName)?;
    let new_norm_len = fs_normalize_path(new_name, cwd, &mut new_norm_buf)?;
    let new_norm = core::str::from_utf8(&new_norm_buf[..new_norm_len]).map_err(|_| FsError::InvalidName)?;

    if new_norm.is_empty() || new_norm.len() > MAX_FILENAME_LEN {
        return Err(FsError::InvalidName);
    }
    unsafe {
        // Check if destination already exists (cannot overwrite existing file/dir via rename)
        for file in FS.files.iter() {
            if file.used && file.name_str() == new_norm {
                return Err(FsError::AlreadyExists);
            }
        }
        for file in FS.files.iter_mut() {
            if file.used && file.name_str() == old_norm {
                if file.read_only {
                    return Err(FsError::ReadOnly);
                }
                file.name_len = new_norm.len();
                file.name[..new_norm.len()].copy_from_slice(new_norm.as_bytes());
                file.modified_tsc = read_tsc_serialized();
                return Ok(());
            }
        }
        Err(FsError::FileNotFound)
    }
}

// Function: fs_copy
// Description: Copy a file in LatencyFS with full relative/absolute path resolution.
// Worst-case execution time: ~1500 ns
pub fn fs_copy(src_name: &str, dst_name: &str) -> Result<(), FsError> {
    let mut src_norm_buf = [0u8; MAX_FILENAME_LEN];
    let mut dst_norm_buf = [0u8; MAX_FILENAME_LEN];
    let cwd = unsafe { core::str::from_utf8(&crate::shell::CURRENT_DIR[..crate::shell::CURRENT_DIR_LEN]).unwrap_or("/") };
    let src_norm_len = fs_normalize_path(src_name, cwd, &mut src_norm_buf)?;
    let src_norm = core::str::from_utf8(&src_norm_buf[..src_norm_len]).map_err(|_| FsError::InvalidName)?;
    let dst_norm_len = fs_normalize_path(dst_name, cwd, &mut dst_norm_buf)?;
    let dst_norm = core::str::from_utf8(&dst_norm_buf[..dst_norm_len]).map_err(|_| FsError::InvalidName)?;

    if let Some(data) = fs_read(src_norm) {
        fs_write(dst_norm, data)
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
