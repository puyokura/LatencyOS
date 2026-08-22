// export_disk.rs - ATA PIO Driver & FAT16 Export Disk (Windows Interop "玄関" Disk)
//
// Real-time classification: Non-real-time offline I/O (Export/Import utility).
// Zero heap allocation: All disk buffers and FAT structures statically pre-allocated.

use crate::serial::{inb, inw, outb, outw};
use crate::serial_println;

pub const ATA_PRIMARY_DATA: u16 = 0x1F0;
pub const ATA_SECONDARY_DATA: u16 = 0x170;

pub const ATA_CMD_READ_SECTORS: u8 = 0x20;
pub const ATA_CMD_WRITE_SECTORS: u8 = 0x30;
pub const ATA_CMD_CACHE_FLUSH: u8 = 0xE7;

pub const ATA_STATUS_BSY: u8 = 0x80;
pub const ATA_STATUS_DRDY: u8 = 0x40;
pub const ATA_STATUS_DRQ: u8 = 0x08;
pub const ATA_STATUS_ERR: u8 = 0x01;

pub const SECTOR_SIZE: usize = 512;
pub const MAX_EXPORT_FILE_SIZE: usize = 65536; // 64 KB static buffer

#[derive(Clone, Copy, Debug)]
pub struct DiskLocation {
    pub base_port: u16,
    pub drive: u8, // 0 = Master, 1 = Slave
    pub detected: bool,
}

static mut EXPORT_DISK_LOC: DiskLocation = DiskLocation {
    base_port: ATA_PRIMARY_DATA,
    drive: 1, // Default to Primary Slave (e.g. index=1 or -hdb)
    detected: false,
};

// Static sector buffer for FAT operations
static mut SECTOR_BUF: [u8; SECTOR_SIZE] = [0; SECTOR_SIZE];

// Pre-allocated static file transfer buffer for import/export
pub static mut FILE_TRANSFER_BUF: [u8; MAX_EXPORT_FILE_SIZE] = [0; MAX_EXPORT_FILE_SIZE];

#[derive(Clone, Copy, Debug)]
pub struct Fat16Bpb {
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub num_fats: u8,
    pub root_dir_entries: u16,
    pub total_sectors: u32,
    pub fat_size_sectors: u16,
    pub root_dir_start_lba: u32,
    pub root_dir_sectors: u32,
    pub data_start_lba: u32,
    pub total_data_clusters: u32,
}

impl Fat16Bpb {
    pub const fn empty() -> Self {
        Self {
            bytes_per_sector: 512,
            sectors_per_cluster: 4,
            reserved_sectors: 4,
            num_fats: 2,
            root_dir_entries: 512,
            total_sectors: 32768,
            fat_size_sectors: 32,
            root_dir_start_lba: 68,
            root_dir_sectors: 32,
            data_start_lba: 100,
            total_data_clusters: 8167,
        }
    }
}

// Function: ata_wait_ready
// Description: Polls until ATA controller is not busy (BSY=0).
// Worst-case execution time: ~100 us
pub fn ata_wait_ready(base_port: u16) -> Result<(), &'static str> {
    let status_port = base_port + 7;
    for _ in 0..100_000 {
        let st = unsafe { inb(status_port) };
        if (st & ATA_STATUS_BSY) == 0 {
            return Ok(());
        }
    }
    Err("ATA disk timeout waiting for ready")
}

// Function: ata_wait_drq
// Description: Polls until ATA controller has data ready (DRQ=1, BSY=0).
// Worst-case execution time: ~200 us
pub fn ata_wait_drq(base_port: u16) -> Result<(), &'static str> {
    let status_port = base_port + 7;
    for _ in 0..100_000 {
        let st = unsafe { inb(status_port) };
        if (st & ATA_STATUS_ERR) != 0 {
            return Err("ATA disk error flag set");
        }
        if (st & ATA_STATUS_BSY) == 0 && (st & ATA_STATUS_DRQ) != 0 {
            return Ok(());
        }
    }
    Err("ATA disk timeout waiting for DRQ")
}

// Function: ata_read_sector
// Description: Reads a single 512-byte sector via 28-bit LBA ATA PIO mode.
// Worst-case execution time: ~25 us
pub fn ata_read_sector(base_port: u16, drive: u8, lba: u32, buf: &mut [u8; SECTOR_SIZE]) -> Result<(), &'static str> {
    ata_wait_ready(base_port)?;

    unsafe {
        // Drive & LBA top 4 bits (0xE0 for Master / 0xF0 for Slave)
        let drive_sel = 0xE0 | ((drive & 1) << 4) | (((lba >> 24) & 0x0F) as u8);
        outb(base_port + 6, drive_sel);

        // 400ns delay by reading status 4 times
        for _ in 0..4 {
            let _ = inb(base_port + 7);
        }

        outb(base_port + 2, 1); // 1 sector
        outb(base_port + 3, (lba & 0xFF) as u8);
        outb(base_port + 4, ((lba >> 8) & 0xFF) as u8);
        outb(base_port + 5, ((lba >> 16) & 0xFF) as u8);
        outb(base_port + 7, ATA_CMD_READ_SECTORS);

        ata_wait_drq(base_port)?;

        for i in 0..256 {
            let word = inw(base_port);
            buf[i * 2] = (word & 0xFF) as u8;
            buf[i * 2 + 1] = ((word >> 8) & 0xFF) as u8;
        }
    }

    Ok(())
}

// Function: ata_write_sector
// Description: Writes a single 512-byte sector via 28-bit LBA ATA PIO mode and flushes cache.
// Worst-case execution time: ~45 us
pub fn ata_write_sector(base_port: u16, drive: u8, lba: u32, buf: &[u8; SECTOR_SIZE]) -> Result<(), &'static str> {
    ata_wait_ready(base_port)?;

    unsafe {
        let drive_sel = 0xE0 | ((drive & 1) << 4) | (((lba >> 24) & 0x0F) as u8);
        outb(base_port + 6, drive_sel);

        for _ in 0..4 {
            let _ = inb(base_port + 7);
        }

        outb(base_port + 2, 1);
        outb(base_port + 3, (lba & 0xFF) as u8);
        outb(base_port + 4, ((lba >> 8) & 0xFF) as u8);
        outb(base_port + 5, ((lba >> 16) & 0xFF) as u8);
        outb(base_port + 7, ATA_CMD_WRITE_SECTORS);

        ata_wait_drq(base_port)?;

        for i in 0..256 {
            let word = (buf[i * 2] as u16) | ((buf[i * 2 + 1] as u16) << 8);
            outw(base_port, word);
        }

        ata_wait_ready(base_port)?;
        outb(base_port + 7, ATA_CMD_CACHE_FLUSH);
        ata_wait_ready(base_port)?;
    }

    Ok(())
}

// Function: parse_bpb
// Description: Validates FAT16 boot sector parameters from Sector 0.
// Worst-case execution time: ~10 us
pub fn parse_bpb(sector0: &[u8; SECTOR_SIZE]) -> Result<Fat16Bpb, &'static str> {
    if sector0[510] != 0x55 || sector0[511] != 0xAA {
        return Err("Invalid boot sector signature (0x55AA missing)");
    }

    let bytes_per_sector = u16::from_le_bytes([sector0[11], sector0[12]]);
    if bytes_per_sector != 512 {
        return Err("Unsupported sector size (only 512 bytes supported)");
    }

    let sectors_per_cluster = sector0[13];
    if sectors_per_cluster == 0 {
        return Err("Invalid sectors per cluster (0)");
    }

    let reserved_sectors = u16::from_le_bytes([sector0[14], sector0[15]]);
    let num_fats = sector0[16];
    let root_dir_entries = u16::from_le_bytes([sector0[17], sector0[18]]);
    let total_sectors_16 = u16::from_le_bytes([sector0[19], sector0[20]]);
    let fat_size_sectors = u16::from_le_bytes([sector0[22], sector0[23]]);
    let total_sectors_32 = u32::from_le_bytes([sector0[32], sector0[33], sector0[34], sector0[35]]);

    let total_sectors = if total_sectors_16 != 0 {
        total_sectors_16 as u32
    } else {
        total_sectors_32
    };

    let root_dir_start_lba = reserved_sectors as u32 + (num_fats as u32 * fat_size_sectors as u32);
    let root_dir_sectors = ((root_dir_entries as u32 * 32) + (bytes_per_sector as u32 - 1)) / bytes_per_sector as u32;
    let data_start_lba = root_dir_start_lba + root_dir_sectors;

    let data_sectors = total_sectors.saturating_sub(data_start_lba);
    let total_data_clusters = data_sectors / sectors_per_cluster as u32;

    Ok(Fat16Bpb {
        bytes_per_sector,
        sectors_per_cluster,
        reserved_sectors,
        num_fats,
        root_dir_entries,
        total_sectors,
        fat_size_sectors,
        root_dir_start_lba,
        root_dir_sectors,
        data_start_lba,
        total_data_clusters,
    })
}

// Function: to_83_name
// Description: Converts a filename string into an 11-byte 8.3 space-padded uppercase name.
// Worst-case execution time: ~5 us
pub fn to_83_name(name: &str) -> [u8; 11] {
    let mut res = [b' '; 11];
    let name_trim = name.trim();
    let base_name = if let Some(idx) = name_trim.rfind('/') {
        &name_trim[idx + 1..]
    } else if let Some(idx) = name_trim.rfind('\\') {
        &name_trim[idx + 1..]
    } else {
        name_trim
    };

    let (stem, ext) = if let Some(dot_idx) = base_name.rfind('.') {
        (&base_name[..dot_idx], &base_name[dot_idx + 1..])
    } else {
        (base_name, "")
    };

    let mut i = 0;
    for b in stem.bytes() {
        if i >= 8 {
            break;
        }
        let upper = if b.is_ascii_lowercase() { b.to_ascii_uppercase() } else { b };
        res[i] = upper;
        i += 1;
    }

    let mut j = 0;
    for b in ext.bytes() {
        if j >= 3 {
            break;
        }
        let upper = if b.is_ascii_lowercase() { b.to_ascii_uppercase() } else { b };
        res[8 + j] = upper;
        j += 1;
    }

    res
}

// Function: export_disk_detect
// Description: Scans ATA ports (Primary Slave -> Primary Master -> Secondary) to detect the Export Disk.
// Worst-case execution time: ~500 us
pub fn export_disk_detect() -> Option<DiskLocation> {
    let candidates = [
        (ATA_PRIMARY_DATA, 1),   // Primary Slave (default index=1)
        (ATA_PRIMARY_DATA, 0),   // Primary Master (index=0 / -hda)
        (ATA_SECONDARY_DATA, 1), // Secondary Slave
        (ATA_SECONDARY_DATA, 0), // Secondary Master
    ];

    for &(base_port, drive) in &candidates {
        unsafe {
            if ata_read_sector(base_port, drive, 0, &mut SECTOR_BUF).is_ok() {
                if parse_bpb(&SECTOR_BUF).is_ok() {
                    let loc = DiskLocation {
                        base_port,
                        drive,
                        detected: true,
                    };
                    EXPORT_DISK_LOC = loc;
                    return Some(loc);
                }
            }
        }
    }

    None
}

// Function: get_export_disk_loc
// Description: Returns current detected disk location or attempts detection.
// Worst-case execution time: ~500 us
pub fn get_export_disk_loc() -> Result<DiskLocation, &'static str> {
    unsafe {
        if EXPORT_DISK_LOC.detected {
            return Ok(EXPORT_DISK_LOC);
        }
    }
    export_disk_detect().ok_or("No FAT16 Export Disk detected on ATA ports (0x1F0/0x170)")
}

// Function: export_disk_get_bpb
// Description: Reads and parses the BPB from the detected disk.
// Worst-case execution time: ~50 us
pub fn export_disk_get_bpb() -> Result<Fat16Bpb, &'static str> {
    let loc = get_export_disk_loc()?;
    unsafe {
        ata_read_sector(loc.base_port, loc.drive, 0, &mut SECTOR_BUF)?;
        parse_bpb(&SECTOR_BUF)
    }
}

// Function: export_disk_list_files
// Description: Lists all files in the root directory of the Export Disk.
// Worst-case execution time: ~1.5 ms
pub fn export_disk_list_files() -> Result<usize, &'static str> {
    let loc = get_export_disk_loc()?;
    let bpb = export_disk_get_bpb()?;

    serial_println!("=== [EXPORT DISK (FAT16) ROOT DIRECTORY] ===");
    serial_println!("  Drive: Port {:#x}, Drive {}", loc.base_port, loc.drive);
    serial_println!("  Total Size: {} KB, Cluster Size: {} Bytes", (bpb.total_sectors * 512) / 1024, bpb.sectors_per_cluster as u32 * 512);
    serial_println!("  -------------------------------------------------------------");
    serial_println!("  FILENAME     | SIZE (Bytes) | CLUSTER | ATTRIBUTES");
    serial_println!("  -------------------------------------------------------------");

    let mut count = 0;

    for sec in 0..bpb.root_dir_sectors {
        let lba = bpb.root_dir_start_lba + sec;
        unsafe {
            ata_read_sector(loc.base_port, loc.drive, lba, &mut SECTOR_BUF)?;
            for entry_idx in 0..(SECTOR_SIZE / 32) {
                let entry = &SECTOR_BUF[entry_idx * 32..(entry_idx + 1) * 32];
                if entry[0] == 0x00 {
                    break;
                }
                if entry[0] == 0xE5 || (entry[11] & 0x0F) == 0x0F || (entry[11] & 0x08) != 0 {
                    continue;
                }

                let mut name_buf = [b' '; 16];
                let mut len = 0;
                for i in 0..8 {
                    if entry[i] != b' ' {
                        name_buf[len] = entry[i];
                        len += 1;
                    }
                }
                if entry[8] != b' ' || entry[9] != b' ' || entry[10] != b' ' {
                    name_buf[len] = b'.';
                    len += 1;
                    for i in 8..11 {
                        if entry[i] != b' ' {
                            name_buf[len] = entry[i];
                            len += 1;
                        }
                    }
                }

                let name_str = core::str::from_utf8(&name_buf[..len]).unwrap_or("<invalid>");
                let first_cluster = u16::from_le_bytes([entry[26], entry[27]]);
                let file_size = u32::from_le_bytes([entry[28], entry[29], entry[30], entry[31]]);
                let attr_str = if (entry[11] & 0x10) != 0 { "DIR" } else { "FILE" };

                serial_println!("  {:12} | {:12} | {:7} | {}", name_str, file_size, first_cluster, attr_str);
                count += 1;
            }
        }
    }

    serial_println!("  -------------------------------------------------------------");
    serial_println!("  Total entries found: {}", count);
    Ok(count)
}

// Function: export_disk_read_file
// Description: Reads a file from the Export Disk by name into the output buffer.
// Worst-case execution time: ~2.5 ms (for 64KB)
pub fn export_disk_read_file(filename: &str, out_buf: &mut [u8]) -> Result<usize, &'static str> {
    let loc = get_export_disk_loc()?;
    let bpb = export_disk_get_bpb()?;
    let target_83 = to_83_name(filename);

    let mut first_cluster = 0u16;
    let mut file_size = 0usize;
    let mut found = false;

    for sec in 0..bpb.root_dir_sectors {
        let lba = bpb.root_dir_start_lba + sec;
        unsafe {
            ata_read_sector(loc.base_port, loc.drive, lba, &mut SECTOR_BUF)?;
            for entry_idx in 0..(SECTOR_SIZE / 32) {
                let entry = &SECTOR_BUF[entry_idx * 32..(entry_idx + 1) * 32];
                if entry[0] == 0x00 {
                    break;
                }
                if entry[0] == 0xE5 || (entry[11] & 0x0F) == 0x0F {
                    continue;
                }

                if entry[0..11] == target_83 {
                    first_cluster = u16::from_le_bytes([entry[26], entry[27]]);
                    file_size = u32::from_le_bytes([entry[28], entry[29], entry[30], entry[31]]) as usize;
                    found = true;
                    break;
                }
            }
        }
        if found {
            break;
        }
    }

    if !found {
        return Err("File not found on Export Disk");
    }

    if file_size == 0 {
        return Ok(0);
    }

    let bytes_to_read = core::cmp::min(file_size, out_buf.len());
    let mut bytes_read = 0;
    let mut current_cluster = first_cluster;
    let _cluster_size_bytes = bpb.sectors_per_cluster as usize * SECTOR_SIZE;

    while current_cluster >= 2 && current_cluster < 0xFFF8 && bytes_read < bytes_to_read {
        let cluster_lba = bpb.data_start_lba + (current_cluster as u32 - 2) * bpb.sectors_per_cluster as u32;

        for s in 0..bpb.sectors_per_cluster as u32 {
            if bytes_read >= bytes_to_read {
                break;
            }
            unsafe {
                ata_read_sector(loc.base_port, loc.drive, cluster_lba + s, &mut SECTOR_BUF)?;
                let chunk = core::cmp::min(SECTOR_SIZE, bytes_to_read - bytes_read);
                out_buf[bytes_read..bytes_read + chunk].copy_from_slice(&SECTOR_BUF[..chunk]);
                bytes_read += chunk;
            }
        }

        // Read next cluster from FAT table
        let fat_sec = bpb.reserved_sectors as u32 + ((current_cluster as u32 * 2) / SECTOR_SIZE as u32);
        let fat_offset = (current_cluster as usize * 2) % SECTOR_SIZE;

        unsafe {
            ata_read_sector(loc.base_port, loc.drive, fat_sec, &mut SECTOR_BUF)?;
            current_cluster = u16::from_le_bytes([SECTOR_BUF[fat_offset], SECTOR_BUF[fat_offset + 1]]);
        }
    }

    Ok(bytes_read)
}

// Function: export_disk_write_file
// Description: Writes data to a file on the Export Disk (creating or updating the entry).
// Worst-case execution time: ~4.0 ms (for 64KB)
pub fn export_disk_write_file(filename: &str, data: &[u8]) -> Result<usize, &'static str> {
    let loc = get_export_disk_loc()?;
    let bpb = export_disk_get_bpb()?;
    let target_83 = to_83_name(filename);

    let cluster_size_bytes = bpb.sectors_per_cluster as usize * SECTOR_SIZE;
    let needed_clusters = if data.is_empty() { 1 } else { (data.len() + cluster_size_bytes - 1) / cluster_size_bytes };

    // 1. Locate free clusters in FAT16
    let mut allocated_clusters = [0u16; 64]; // Support up to 64 clusters (~128KB)
    if needed_clusters > allocated_clusters.len() {
        return Err("File size exceeds static cluster allocation limit (128KB)");
    }

    let mut found_clusters = 0;
    for c in 2..(bpb.total_data_clusters as u16 + 2) {
        let fat_sec = bpb.reserved_sectors as u32 + ((c as u32 * 2) / SECTOR_SIZE as u32);
        let fat_offset = (c as usize * 2) % SECTOR_SIZE;

        unsafe {
            ata_read_sector(loc.base_port, loc.drive, fat_sec, &mut SECTOR_BUF)?;
            let val = u16::from_le_bytes([SECTOR_BUF[fat_offset], SECTOR_BUF[fat_offset + 1]]);
            if val == 0x0000 {
                allocated_clusters[found_clusters] = c;
                found_clusters += 1;
                if found_clusters == needed_clusters {
                    break;
                }
            }
        }
    }

    if found_clusters < needed_clusters {
        return Err("Not enough free space on Export Disk");
    }

    // 2. Write data to allocated clusters
    let mut bytes_written = 0;
    for (_i, &cluster) in allocated_clusters[..needed_clusters].iter().enumerate() {
        let cluster_lba = bpb.data_start_lba + (cluster as u32 - 2) * bpb.sectors_per_cluster as u32;

        for s in 0..bpb.sectors_per_cluster as u32 {
            unsafe {
                let chunk = if bytes_written < data.len() {
                    core::cmp::min(SECTOR_SIZE, data.len() - bytes_written)
                } else {
                    0
                };

                SECTOR_BUF.fill(0);
                if chunk > 0 {
                    SECTOR_BUF[..chunk].copy_from_slice(&data[bytes_written..bytes_written + chunk]);
                    bytes_written += chunk;
                }
                ata_write_sector(loc.base_port, loc.drive, cluster_lba + s, &SECTOR_BUF)?;
            }
        }
    }

    // 3. Update FAT tables (FAT1 & FAT2)
    for (i, &cluster) in allocated_clusters[..needed_clusters].iter().enumerate() {
        let next_val: u16 = if i + 1 < needed_clusters {
            allocated_clusters[i + 1]
        } else {
            0xFFFF // EOF marker
        };

        let fat1_sec = bpb.reserved_sectors as u32 + ((cluster as u32 * 2) / SECTOR_SIZE as u32);
        let fat_offset = (cluster as usize * 2) % SECTOR_SIZE;

        unsafe {
            ata_read_sector(loc.base_port, loc.drive, fat1_sec, &mut SECTOR_BUF)?;
            SECTOR_BUF[fat_offset..fat_offset + 2].copy_from_slice(&next_val.to_le_bytes());
            ata_write_sector(loc.base_port, loc.drive, fat1_sec, &SECTOR_BUF)?;

            // Also mirror to FAT2
            let fat2_sec = fat1_sec + bpb.fat_size_sectors as u32;
            ata_write_sector(loc.base_port, loc.drive, fat2_sec, &SECTOR_BUF)?;
        }
    }

    // 4. Update Root Directory Entry
    let mut entry_saved = false;
    for sec in 0..bpb.root_dir_sectors {
        let lba = bpb.root_dir_start_lba + sec;
        unsafe {
            ata_read_sector(loc.base_port, loc.drive, lba, &mut SECTOR_BUF)?;
            for entry_idx in 0..(SECTOR_SIZE / 32) {
                let entry_offset = entry_idx * 32;
                let entry = &mut SECTOR_BUF[entry_offset..entry_offset + 32];

                // Check for matching name (overwrite) or free slot (0x00 or 0xE5)
                if entry[0..11] == target_83 || entry[0] == 0x00 || entry[0] == 0xE5 {
                    entry[0..11].copy_from_slice(&target_83);
                    entry[11] = 0x20; // Attribute: Archive
                    entry[12..26].fill(0);
                    entry[26..28].copy_from_slice(&(allocated_clusters[0]).to_le_bytes());
                    entry[28..32].copy_from_slice(&(data.len() as u32).to_le_bytes());

                    ata_write_sector(loc.base_port, loc.drive, lba, &SECTOR_BUF)?;
                    entry_saved = true;
                    break;
                }
            }
        }
        if entry_saved {
            break;
        }
    }

    if !entry_saved {
        return Err("Root directory is full on Export Disk");
    }

    Ok(data.len())
}
