// net.rs - Zero-Allocation Native SRTP (AES-128-GCM) & Delay-Based Congestion Controller
//
// Worst-case execution time: Documented per function.

#![allow(static_mut_refs)]

use crate::crypto::{aes_128_gcm_encrypt, Aes128Key};
use crate::e1000::{poll_rx_packet, send_packet, E1000};
use crate::gpu::FrameHandle;
use crate::tsc::{read_tsc_serialized, tsc_to_ns};
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};

pub const STATIC_MASTER_KEY: [u8; 16] = [
    0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF,
    0xFE, 0xDC, 0xBA, 0x98, 0x76, 0x54, 0x32, 0x10,
];

pub const STATIC_MASTER_SALT: [u8; 12] = [
    0xA5, 0x5A, 0xA5, 0x5A, 0xA5, 0x5A, 0xA5, 0x5A, 0xA5, 0x5A, 0xA5, 0x5A,
];

pub const RTP_PAYLOAD_TYPE_VIDEO: u8 = 96;
pub const RTP_SSRC_VIDEO: u32 = 0x1234_5678;
pub const MAX_SRTP_PAYLOAD_CHUNK: usize = 1380; // Fits within 1500 MTU with IP/UDP/SRTP/GMAC headers

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendError {
    DeadlineExceeded,
    NicQueueFull,
    #[allow(dead_code)]
    InvalidFrame,
}

// Global Congestion Controller State (Lock-Free Atomics)
pub static MIN_RTT_NS: AtomicU64 = AtomicU64::new(50_000); // 50 us initial baseline
pub static LAST_RTT_NS: AtomicU64 = AtomicU64::new(50_000);
pub static DELTA_DELAY_NS: AtomicU64 = AtomicU64::new(0);
pub static CONGESTION_RATE_PCT: AtomicU8 = AtomicU8::new(100);
pub static CONSECUTIVE_GOOD_FRAMES: AtomicU32 = AtomicU32::new(0);
pub static TOTAL_PACKETS_SENT: AtomicU64 = AtomicU64::new(0);
pub static TOTAL_FRAMES_DROPPED: AtomicU64 = AtomicU64::new(0);
pub static TOTAL_ACKS_RECEIVED: AtomicU64 = AtomicU64::new(0);

pub const CONGESTION_THRESHOLD_NS: u64 = 200_000; // 200 us (0.2 ms) threshold

#[allow(dead_code)]
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct EthernetHeader {
    pub dst_mac: [u8; 6],
    pub src_mac: [u8; 6],
    pub ether_type: u16,
}

#[allow(dead_code)]
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Ipv4Header {
    pub ver_ihl: u8,
    pub tos: u8,
    pub total_len: u16,
    pub id: u16,
    pub flags_frag: u16,
    pub ttl: u8,
    pub protocol: u8,
    pub checksum: u16,
    pub src_ip: [u8; 4],
    pub dst_ip: [u8; 4],
}

#[allow(dead_code)]
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct UdpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub length: u16,
    pub checksum: u16,
}

#[allow(dead_code)]
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SrtpHeader {
    pub v_p_x_cc: u8,
    pub m_pt: u8,
    pub seq_num: u16,
    pub timestamp: u32,
    pub ssrc: u32,
}

#[allow(dead_code)]
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct AckPacketPayload {
    pub magic: u32, // 0x41434B31 ("ACK1")
    pub acked_seq: u16,
    pub reserved: u16,
    pub t_pkt_tx: u64,
    pub rx_timestamp: u64,
}

// Function: compute_ip_checksum
// Description: Compute IPv4 header checksum.
// Worst-case execution time: ~20 ns
pub fn compute_ip_checksum(header: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i < header.len() {
        let word = ((header[i] as u32) << 8) | (header[i + 1] as u32);
        sum = sum.wrapping_add(word);
        i += 2;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

// Function: on_ack_received
// Description: Update delay-based congestion control state upon receiving an ACK packet.
// Worst-case execution time: ~35 ns
pub fn on_ack_received(t_pkt_tx: u64, freq_hz: u64) {
    let now_tsc = read_tsc_serialized();
    if now_tsc < t_pkt_tx {
        return;
    }

    let rtt_ns = tsc_to_ns(now_tsc - t_pkt_tx, freq_hz);
    LAST_RTT_NS.store(rtt_ns, Ordering::Relaxed);
    TOTAL_ACKS_RECEIVED.fetch_add(1, Ordering::Relaxed);

    let mut min_rtt = MIN_RTT_NS.load(Ordering::Relaxed);
    if rtt_ns < min_rtt || min_rtt == 0 {
        MIN_RTT_NS.store(rtt_ns, Ordering::Relaxed);
        min_rtt = rtt_ns;
    }

    let delta_delay = rtt_ns.saturating_sub(min_rtt);
    DELTA_DELAY_NS.store(delta_delay, Ordering::Relaxed);

    if delta_delay > CONGESTION_THRESHOLD_NS {
        // Congestion detected: Multiplicative Decrease (80% of current rate)
        let curr = CONGESTION_RATE_PCT.load(Ordering::Relaxed);
        let new_rate = ((curr as u32 * 80) / 100) as u8;
        CONGESTION_RATE_PCT.store(core::cmp::max(20, new_rate), Ordering::Relaxed);
        CONSECUTIVE_GOOD_FRAMES.store(0, Ordering::Relaxed);
    } else {
        // Normal condition: 4 consecutive good frames -> Additive Increase (+5%)
        let good = CONSECUTIVE_GOOD_FRAMES.fetch_add(1, Ordering::Relaxed) + 1;
        if good >= 4 {
            let curr = CONGESTION_RATE_PCT.load(Ordering::Relaxed);
            let new_rate = core::cmp::min(100, curr.saturating_add(5));
            CONGESTION_RATE_PCT.store(new_rate, Ordering::Relaxed);
            CONSECUTIVE_GOOD_FRAMES.store(0, Ordering::Relaxed);
        }
    }
}

// Function: poll_rx_ack
// Description: Poll incoming e1000 RX ring buffer for RTCP/ACK packet and process RTT calculation.
// Worst-case execution time: ~90 ns
pub fn poll_rx_ack(freq_hz: u64) -> bool {
    let mut rx_buf = [0u8; 128];
    if let Some(len) = poll_rx_packet(&mut rx_buf) {
        // Minimum Ethernet + IP + UDP + ACK Payload = 14 + 20 + 8 + 24 = 66 bytes
        if len >= 66 {
            let payload_offset = 14 + 20 + 8;
            let ack_bytes = &rx_buf[payload_offset..payload_offset + 24];
            let magic = u32::from_be_bytes([ack_bytes[0], ack_bytes[1], ack_bytes[2], ack_bytes[3]]);
            if magic == 0x41434B31 { // "ACK1"
                let t_tx = u64::from_be_bytes([
                    ack_bytes[8], ack_bytes[9], ack_bytes[10], ack_bytes[11],
                    ack_bytes[12], ack_bytes[13], ack_bytes[14], ack_bytes[15],
                ]);
                on_ack_received(t_tx, freq_hz);
                return true;
            }
        }
    }
    false
}

// Function: stream_send_frame
// Description: Packetize, AES-128-GCM encrypt, and transmit an encoded video frame over e1000 NIC with strict deadline enforcement.
// Worst-case execution time: ~31_000 ns (for 45 MTU packets / 64KB frame)
pub fn stream_send_frame(
    frame: &FrameHandle,
    deadline_tsc: u64,
    packet_seq: &mut u16,
) -> Result<usize, SendError> {
    // 1. Deadline Enforcement: If deadline is already passed, drop frame immediately
    let current_tsc = read_tsc_serialized();
    if current_tsc > deadline_tsc {
        TOTAL_FRAMES_DROPPED.fetch_add(1, Ordering::Relaxed);
        return Err(SendError::DeadlineExceeded);
    }

    let src_mac = unsafe {
        match E1000.as_ref() {
            Some(d) => d.mac,
            None => [0x52, 0x54, 0x00, 0x12, 0x34, 0x56],
        }
    };
    let dst_mac = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]; // Broadcast or peer MAC

    let key = Aes128Key::new(&STATIC_MASTER_KEY);

    if frame.phys_addr < 0x1000 || frame.size == 0 || frame.size > 16 * 1024 * 1024 {
        return Ok(0);
    }

    // Frame data slice (zero-copy pointer)
    let frame_slice = unsafe {
        core::slice::from_raw_parts(frame.phys_addr as *const u8, frame.size)
    };

    let mut bytes_sent = 0;
    let mut offset = 0;

    let mut packet_buffer = [0u8; 1514];

    while offset < frame_slice.len() {
        // Re-check deadline before transmitting each packet burst
        if read_tsc_serialized() > deadline_tsc {
            TOTAL_FRAMES_DROPPED.fetch_add(1, Ordering::Relaxed);
            return Err(SendError::DeadlineExceeded);
        }

        let chunk_len = core::cmp::min(MAX_SRTP_PAYLOAD_CHUNK, frame_slice.len() - offset);

        let eth_hdr_len = 14;
        let ip_hdr_len = 20;
        let udp_hdr_len = 8;
        let srtp_hdr_len = 12;
        let tag_len = 16;

        let total_packet_len = eth_hdr_len + ip_hdr_len + udp_hdr_len + srtp_hdr_len + chunk_len + tag_len;

        // 1. Ethernet Header
        packet_buffer[0..6].copy_from_slice(&dst_mac);
        packet_buffer[6..12].copy_from_slice(&src_mac);
        packet_buffer[12..14].copy_from_slice(&0x0800u16.to_be_bytes()); // EtherType: IPv4

        // 2. IPv4 Header
        let ip_total_len = (ip_hdr_len + udp_hdr_len + srtp_hdr_len + chunk_len + tag_len) as u16;
        packet_buffer[14] = 0x45; // Version 4, IHL 5
        packet_buffer[15] = 0x00; // DSCP/ECN
        packet_buffer[16..18].copy_from_slice(&ip_total_len.to_be_bytes());
        packet_buffer[18..20].copy_from_slice(&frame.frame_id.to_be_bytes()[6..8]); // ID
        packet_buffer[20..22].copy_from_slice(&0x4000u16.to_be_bytes()); // Don't Fragment
        packet_buffer[22] = 64; // TTL
        packet_buffer[23] = 17; // Protocol: UDP
        packet_buffer[24..26].copy_from_slice(&0u16.to_be_bytes()); // Clear Checksum
        packet_buffer[26..30].copy_from_slice(&[192, 168, 1, 10]); // Src IP
        packet_buffer[30..34].copy_from_slice(&[192, 168, 1, 20]); // Dst IP

        let ip_csum = compute_ip_checksum(&packet_buffer[14..34]);
        packet_buffer[24..26].copy_from_slice(&ip_csum.to_be_bytes());

        // 3. UDP Header
        let udp_len = (udp_hdr_len + srtp_hdr_len + chunk_len + tag_len) as u16;
        packet_buffer[34..36].copy_from_slice(&5004u16.to_be_bytes()); // Src Port
        packet_buffer[36..38].copy_from_slice(&5004u16.to_be_bytes()); // Dst Port
        packet_buffer[38..40].copy_from_slice(&udp_len.to_be_bytes());
        packet_buffer[40..42].copy_from_slice(&0u16.to_be_bytes()); // UDP Checksum (0 = disabled)

        // 4. SRTP Header (RTP Base Header)
        let srtp_offset = 42;
        let seq = *packet_seq;
        *packet_seq = packet_seq.wrapping_add(1);

        packet_buffer[srtp_offset] = 0x80; // V=2, P=0, X=0, CC=0
        packet_buffer[srtp_offset + 1] = RTP_PAYLOAD_TYPE_VIDEO;
        packet_buffer[srtp_offset + 2..srtp_offset + 4].copy_from_slice(&seq.to_be_bytes());
        let rtp_ts = (frame.vblank_tsc & 0xFFFF_FFFF) as u32;
        packet_buffer[srtp_offset + 4..srtp_offset + 8].copy_from_slice(&rtp_ts.to_be_bytes());
        packet_buffer[srtp_offset + 8..srtp_offset + 12].copy_from_slice(&RTP_SSRC_VIDEO.to_be_bytes());

        // 5. Payload Copy & AES-128-GCM In-Place Encryption
        let payload_offset = srtp_offset + srtp_hdr_len;
        packet_buffer[payload_offset..payload_offset + chunk_len]
            .copy_from_slice(&frame_slice[offset..offset + chunk_len]);

        // Construct 12-byte IV = Salt ^ (SSRC || ROC || Seq)
        let mut iv = STATIC_MASTER_SALT;
        iv[4] ^= ((RTP_SSRC_VIDEO >> 24) & 0xFF) as u8;
        iv[5] ^= ((RTP_SSRC_VIDEO >> 16) & 0xFF) as u8;
        iv[6] ^= ((RTP_SSRC_VIDEO >> 8) & 0xFF) as u8;
        iv[7] ^= (RTP_SSRC_VIDEO & 0xFF) as u8;
        iv[10] ^= ((seq >> 8) & 0xFF) as u8;
        iv[11] ^= (seq & 0xFF) as u8;

        let mut aad = [0u8; 12];
        aad.copy_from_slice(&packet_buffer[srtp_offset..srtp_offset + srtp_hdr_len]);
        let mut tag = [0u8; 16];

        aes_128_gcm_encrypt(
            &key,
            &iv,
            &aad,
            &mut packet_buffer[payload_offset..payload_offset + chunk_len],
            &mut tag,
        );

        // Append 16-byte GMAC Authentication Tag
        let tag_offset = payload_offset + chunk_len;
        packet_buffer[tag_offset..tag_offset + 16].copy_from_slice(&tag);

        // 6. Transmit via e1000 PMD
        if send_packet(&packet_buffer[..total_packet_len]).is_err() {
            return Err(SendError::NicQueueFull);
        }

        TOTAL_PACKETS_SENT.fetch_add(1, Ordering::Relaxed);
        bytes_sent += total_packet_len;
        offset += chunk_len;
    }

    Ok(bytes_sent)
}
