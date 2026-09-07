# VMware / VirtualBox / Physical Hardware Execution Guide

LatencyOS provides a standalone, hybrid bootable ISO (`dist/LatencyOS.iso`) supporting both **UEFI (x86_64)** and **Legacy BIOS** via the Limine bootloader protocol.

---

## 1. Generating the Bootable ISO

To generate the ISO, run:

```powershell
# Build kernel, host compiler, and generate dist/LatencyOS.iso
cargo xtask iso --release

# Or package all distribution targets (LatencyOS.exe, pulc.exe, LatencyOS.iso)
cargo xtask dist
```

Generated output:
- `dist/LatencyOS.iso`: Hybrid bootable ISO image (El Torito BIOS + UEFI System Partition).

To verify that the ISO boots cleanly inside QEMU:
```powershell
cargo xtask test-iso
```

---

## 2. Running on VMware Workstation / Player

### Step 1: Create a New Virtual Machine
1. Open VMware Workstation / Player.
2. Select **File -> New Virtual Machine** (Typical / Recommended).
3. Select **Installer disc image file (iso)**, and browse to `dist/LatencyOS.iso`.
4. Guest Operating System:
   - **OS**: `Other`
   - **Version**: `Other 64-bit` (or `Other Linux 5.x kernel 64-bit`).
5. Set Virtual Machine Name (e.g. `LatencyOS`).
6. Disk Capacity: Any size (e.g. 1 GB or 2 GB, LatencyOS runs completely in-memory from RAM).

### Step 2: Configure VM Hardware
Click **Customize Hardware**:
- **Memory**: Minimum `128 MB` (Recommended: `512 MB` or `1024 MB`).
- **Processors**: `4` Cores (LatencyOS is designed for a dedicated 4-core real-time pipeline).
  - Check `Enable VT-x/AMD-V` or Virtualize CPU performance counters if available.
- **Network Adapter**: NAT or Bridged.
  - LatencyOS includes an integrated poll-mode driver for the Intel 82540EM (e1000) NIC.
  - In `.vmx` configuration, ensure: `ethernet0.virtualDev = "e1000"`.
- **Display**: Standard VGA.

### Step 3: Booting
1. Power on the Virtual Machine.
2. The Limine bootloader will show the boot menu:
   - `LatencyOS (x86_64 ELF)`
3. Press Enter (or wait 3 seconds for auto-boot).
4. Core 0 will boot the system, initialize SMP Cores 1–3, configure the real-time pipeline, and present the Pulse Shell prompt:
   ```text
   LatencyOS 0.0.5 (x86_64 hard-realtime)
   [c0|18ns] %
   ```

---

## 3. Running on Oracle VirtualBox

### Step 1: Create Virtual Machine
1. Open VirtualBox -> **New**.
2. **Name**: `LatencyOS`.
3. **Type**: `Other`.
4. **Version**: `Other/Unknown (64-bit)`.
5. **Base Memory**: `512 MB` (or higher).
6. **Processors**: `4` CPUs.
7. **Hard disk**: Optional (LatencyOS can boot diskless, or attach an IDE/SATA disk for export/import).

### Step 2: Storage & Network Settings
1. Go to **Settings -> Storage**.
2. Under Storage Devices, select the Optical Drive and choose `dist/LatencyOS.iso`.
3. Go to **Settings -> Network -> Adapter 1**:
   - Attached to: `NAT` (or `Bridged`).
   - Advanced -> Adapter Type: `Intel PRO/1000 MT Desktop (82540EM)` (compatible with e1000).

### Step 3: Booting
1. Start the Virtual Machine.
2. Select `LatencyOS` in the Limine bootloader.
3. System boots directly into the interactive Pulse Shell.

---

## 4. Running on Physical Hardware (Bare Metal via USB)

`dist/LatencyOS.iso` is built as an **isohybrid** image. It can be written directly to a USB flash drive (raw block write):

### On Windows (Rufus):
1. Insert a USB drive.
2. Open **Rufus**.
3. Select Device (USB drive) and Boot selection (`LatencyOS.iso`).
4. Select **Write in DD Image mode** when prompted.

### On Linux / macOS (`dd`):
```bash
sudo dd if=dist/LatencyOS.iso of=/dev/sdX bs=4M status=progress conv=fsync
```
*(Replace `/dev/sdX` with your target USB block device).*

Boot the target PC via UEFI or Legacy BIOS boot menu (F11/F12/Esc).
Ensure CPU supports x86_64, SSE4.2, and AES-NI.
