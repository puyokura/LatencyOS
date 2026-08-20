// pci.rs - PCI Bus Enumeration and MMIO Base Address Discovery
//
// Worst-case execution time: Documented per function.

pub const PCI_CONFIG_ADDRESS: u16 = 0x0CF8;
pub const PCI_CONFIG_DATA: u16 = 0x0CFC;

// Function: inl
// Description: Read 32-bit dword from I/O port.
// Worst-case execution time: ~15 ns
#[inline]
pub unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    core::arch::asm!(
        "in eax, dx",
        out("eax") val,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
    );
    val
}

// Function: outl
// Description: Write 32-bit dword to I/O port.
// Worst-case execution time: ~15 ns
#[inline]
pub unsafe fn outl(port: u16, val: u32) {
    core::arch::asm!(
        "out dx, eax",
        in("dx") port,
        in("eax") val,
        options(nomem, nostack, preserves_flags)
    );
}

// Function: pci_read_config_32
// Description: Read 32-bit register from PCI Configuration Space.
// Worst-case execution time: ~35 ns
pub fn pci_read_config_32(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let address = ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC)
        | 0x8000_0000;

    unsafe {
        outl(PCI_CONFIG_ADDRESS, address);
        inl(PCI_CONFIG_DATA)
    }
}

// Function: pci_write_config_32
// Description: Write 32-bit register to PCI Configuration Space.
// Worst-case execution time: ~35 ns
pub fn pci_write_config_32(bus: u8, slot: u8, func: u8, offset: u8, val: u32) {
    let address = ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC)
        | 0x8000_0000;

    unsafe {
        outl(PCI_CONFIG_ADDRESS, address);
        outl(PCI_CONFIG_DATA, val);
    }
}

// Function: pci_read_config_16
// Description: Read 16-bit word from PCI Configuration Space.
// Worst-case execution time: ~35 ns
pub fn pci_read_config_16(bus: u8, slot: u8, func: u8, offset: u8) -> u16 {
    let dword = pci_read_config_32(bus, slot, func, offset);
    ((dword >> ((offset & 2) * 8)) & 0xFFFF) as u16
}

// Function: pci_write_config_16
// Description: Write 16-bit word to PCI Configuration Space.
// Worst-case execution time: ~50 ns
pub fn pci_write_config_16(bus: u8, slot: u8, func: u8, offset: u8, val: u16) {
    let shift = (offset & 2) * 8;
    let mask = !(0xFFFF << shift);
    let current = pci_read_config_32(bus, slot, func, offset);
    let new_val = (current & mask) | ((val as u32) << shift);
    pci_write_config_32(bus, slot, func, offset, new_val);
}

#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    pub bus: u8,
    pub slot: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub bar0: u64,
}

// Function: find_e1000_device
// Description: Scans PCI bus for Intel e1000 / 82540EM network adapter and enables Bus Mastering.
// Worst-case execution time: ~15_000 ns (scanning up to 32 slots)
pub fn find_e1000_device() -> Option<PciDevice> {
    for bus in 0..=2 {
        for slot in 0..32 {
            let vendor_id = pci_read_config_16(bus, slot, 0, 0x00);
            if vendor_id == 0xFFFF || vendor_id == 0x0000 {
                continue;
            }

            let device_id = pci_read_config_16(bus, slot, 0, 0x02);

            // Intel Vendor ID = 0x8086
            // e1000 Device IDs: 0x100E (82540EM), 0x100F, 0x1004, 0x10D3, 0x107C
            if vendor_id == 0x8086 && (device_id == 0x100E || device_id == 0x100F || device_id == 0x1004 || device_id == 0x10D3 || device_id == 0x107C) {
                // Enable Bus Master (bit 2) and Memory Space (bit 1) in Command register (0x04)
                let cmd = pci_read_config_16(bus, slot, 0, 0x04);
                pci_write_config_16(bus, slot, 0, 0x04, cmd | 0x0006);

                // Read BAR0 (MMIO physical address)
                let bar0_low = pci_read_config_32(bus, slot, 0, 0x10);
                let bar0 = (bar0_low & 0xFFFF_FFF0) as u64;

                return Some(PciDevice {
                    bus,
                    slot,
                    func: 0,
                    vendor_id,
                    device_id,
                    bar0,
                });
            }
        }
    }
    None
}
