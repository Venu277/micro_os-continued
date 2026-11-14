# Step 6: LAN Networking (Intel e1000 & smoltcp)

## Executive Summary & Objectives

Step 6 introduces bare-metal local-area network (LAN) communication to MicroOS. We implemented a hardware driver for the ubiquitous Intel 82540EM / 82545EM Gigabit Ethernet Controller (e1000) utilizing DMA descriptor rings, coupled with `smoltcp`, an event-driven, zero-allocation network stack designed for bare-metal systems.

### Key Objectives Achieved:
1. **PCI Bus Probing & Configuration**: Enhanced `src/pci.rs` to discover Intel e1000 controllers (PCI Device IDs `0x100E` for QEMU and `0x100F` for VMware Workstation), read 32-bit/64-bit Memory-Mapped I/O (MMIO) Base Address Registers (BAR0), and enable PCI Bus Mastering for direct memory access (DMA).
2. **Intel e1000 Hardware Driver (`src/net/e1000.rs`)**:
   - Mapped controller MMIO space into kernel paging table.
   - Initialized aligned circular DMA descriptor rings (16 Tx descriptors, 32 Rx descriptors) backed by static 2048-byte packet buffers.
   - Translated virtual buffer addresses to physical frames via `mapper.translate_addr()` for hardware DMA access.
   - Configured e1000 control registers: Device Reset, Link Up (SLU), Receiver Control (`RCTL` with Broadcast Enable, Multicast Promiscuous, 2048-byte buffers), and Transmit Control (`TCTL` with Collision Threshold/Distance and Padding).
   - Read MAC address from EEPROM/RAL and restored Receive Address Registers with Address Valid (`AV`) bit.
3. **smoltcp Protocol Stack Integration (`src/net/mod.rs`)**:
   - Implemented `smoltcp::phy::Device` for `E1000Phy` wrapper providing zero-copy `RxToken` and `TxToken`.
   - Initialized smoltcp `Interface` with static IPv4 configuration (`10.0.2.15/24`) and default gateway route (`10.0.2.2`).
   - Managed `SocketSet` supporting ICMPv4 (raw socket) and TCP (`TcpSocket`).
   - Implemented synchronous ICMP Echo Request/Reply `ping()` with millisecond RTT measurement and packet timeout.
   - Implemented bare-metal TCP HTTP/1.0 GET client for `curl`.
4. **Interactive Shell Commands (`src/shell.rs`)**:
   - `ifconfig`: Displays interface name, MAC address, IPv4 address, subnet mask, default gateway, and Tx/Rx packet/byte statistics.
   - `ping <ip>`: Sends ICMP Echo Requests to the target IPv4 address with RTT calculation.
   - `curl <ip> [port] [path]`: Connects via TCP, issues HTTP GET, and prints response headers and body.
5. **Comprehensive Integration Testing (`tests/networking.rs`)**:
   - Tested IPv4 string parsing and octet validation.
   - Verified 16-byte size and 16-byte alignment of hardware DMA `RxDesc` and `TxDesc` structures.
   - Verified ICMP packet formatting, checksum calculation, and payload integrity.
   - Verified full end-to-end e1000 hardware initialization and gateway ARP/ping in QEMU.

---

## Architecture & Design

### Network Subsystem Architecture

```
+-------------------------------------------------------------------------+
|                              MicroOS Shell                              |
|           [ifconfig]            [ping <ip>]           [curl <ip>]       |
+-------------------------------------------------------------------------+
                                     |
+-------------------------------------------------------------------------+
|                  NetworkStack (src/net/mod.rs)                          |
|  - smoltcp::iface::Interface (Ethernet, IPv4 10.0.2.15/24, GW 10.0.2.2)  |
|  - smoltcp::iface::SocketSet (IcmpSocket, TcpSocket)                    |
|  - Address Resolution Protocol (ARP) Cache Table                        |
+-------------------------------------------------------------------------+
                                     |
+-------------------------------------------------------------------------+
|                    E1000Phy (smoltcp::phy::Device)                      |
|  - RxToken: Consumes received frames from ring buffers                   |
|  - TxToken: Emits transmitted frames into descriptor ring               |
+-------------------------------------------------------------------------+
                                     |
+-------------------------------------------------------------------------+
|                    E1000Driver (src/net/e1000.rs)                       |
|  - MMIO Registers: CTRL, STATUS, RCTL, TCTL, RDBAL/H, TDBAL/H, RDT, TDT |
|  - DMA Tx Ring: 16 Descriptors (16B each) -> 16x 2048B Buffers         |
|  - DMA Rx Ring: 32 Descriptors (16B each) -> 32x 2048B Buffers         |
+-------------------------------------------------------------------------+
                                     |
+-------------------------------------------------------------------------+
|                 PCI Bus & Hardware (QEMU / VMware)                      |
|  - Intel 82540EM (0x8086:0x100E) / Intel 82545EM (0x8086:0x100F)       |
|  - Virtual / Physical MMIO Base Address Register (BAR0)                 |
+-------------------------------------------------------------------------+
```

### DMA Ring Buffer Architecture

The Intel e1000 controller operates via circular descriptor rings allocated in physical memory accessible to both the CPU and the PCI bus master:

- **Receive Descriptors (`RxDesc`, 16 bytes)**:
  - `buffer_addr: u64`: Physical address of the 2048-byte receive buffer.
  - `length: u16`: Populated by hardware with received frame byte count.
  - `checksum: u16`: Hardware checksum offload.
  - `status: u8`: Bit 0 = Descriptor Done (`DD`), Bit 1 = End of Packet (`EOP`).
  - `errors: u8`: CRC/frame errors.
  - `special: u16`: VLAN tag.
- **Transmit Descriptors (`TxDesc`, 16 bytes)**:
  - `buffer_addr: u64`: Physical address of the packet data to transmit.
  - `length: u16`: Length of data buffer (up to 2048 bytes).
  - `cso: u8`: Checksum offset.
  - `cmd: u8`: Command flags (Bit 0 = `EOP`, Bit 1 = `IFCS` Insert FCS/CRC, Bit 3 = `RS` Report Status).
  - `status: u8`: Bit 0 = Descriptor Done (`DD`).
  - `css: u8`: Checksum start.
  - `special: u16`: Reserved / VLAN.

---

## Implementation Details

### 1. PCI Enumeration & BAR Probing (`src/pci.rs`)

We added `read_bar()` and `enable_bus_mastering()` to inspect PCI configuration headers:
```rust
pub fn find_e1000() -> Option<(u8, u8, u8, u32)> {
    for bus in 0..=255 {
        for device in 0..32 {
            for function in 0..8 {
                let vendor = read_config_word(bus, device, function, 0x00);
                if vendor == 0xFFFF { if function == 0 { break; } continue; }
                let dev_id = read_config_word(bus, device, function, 0x02);
                if vendor == 0x8086 && (dev_id == 0x100E || dev_id == 0x100F) {
                    let bar0 = read_bar(bus, device, function, 0);
                    return Some((bus, device, function, bar0));
                }
            }
        }
    }
    None
}
```

### 2. e1000 Initialization Sequence (`src/net/e1000.rs`)

1. **Page Table Mapping**: The physical MMIO address (e.g. `0xFEB80000`) is mapped to a dedicated virtual address (`0x_3333_0000_0000`) with `PRESENT | WRITABLE | NO_CACHE`.
2. **Device Reset**: Set bit 26 (`CTRL_RST`) in `REG_CTRL`. Wait 10,000 iterations for hardware reset to complete. Set Set Link Up (`CTRL_SLU`, bit 6).
3. **Descriptor Allocation & Translation**: DMA rings and packet buffers are statically allocated in `DMA_RING`. Physical addresses are resolved via `mapper.translate_addr()`.
4. **Tx Ring Setup**: Write physical address to `REG_TDBAL`/`REG_TDBAH`. Set `REG_TDLEN` to `16 * 16 = 256 bytes`. Set `REG_TDH = 0` and `REG_TDT = 0`. Configure `REG_TCTL` with `TCTL_EN | TCTL_PSP | (0x10 << 4) | (0x40 << 12)`.
5. **Rx Ring Setup**: Write physical address to `REG_RDBAL`/`REG_RDBAH`. Set `REG_RDLEN` to `32 * 16 = 512 bytes`. Set `REG_RDH = 0` and `REG_RDT = 31`. Configure `REG_RCTL` with `RCTL_EN | RCTL_BAM | RCTL_MPE | RCTL_SZ_2048 | RCTL_SECRC`.
6. **MAC Filtering**: Read MAC address from EEPROM (or `REG_RAL0`/`REG_RAH0`). Reprogram `REG_RAL0` and `REG_RAH0` with `RAH_AV` (Address Valid, bit 31).

### 3. smoltcp PHY Layer & Network Stack (`src/net/mod.rs`)

`E1000Phy` provides zero-allocation framing between the e1000 descriptor ring and smoltcp's network engine:
```rust
impl Device for E1000Phy {
    type RxToken<'a> = RxToken where Self: 'a;
    type TxToken<'a> = TxToken<'a> where Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let mut dev = self.0.lock();
        if let Some(packet) = dev.receive() {
            Some((RxToken { packet }, TxToken { driver: self.0 }))
        } else {
            None
        }
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(TxToken { driver: self.0 })
    }
}
```

---

## Verification & Test Results

### 1. Automated Integration Tests (`tests/networking.rs`)

The `tests/networking.rs` test suite validates:
- `networking::parse_ipv4`: Validates standard IPv4 dotted-quad strings, rejects malformed inputs and values > 255.
- `networking::e1000_descriptors`: Validates that `RxDesc` and `TxDesc` match the exact 16-byte size and 16-byte alignment required by the Intel PCI specification.
- `networking::icmp_formatting`: Validates that ICMP Echo Requests serialize correctly and pass RFC 792 checksum verification.
- `networking::init_and_ping_gateway`: Discovers the e1000 device, initializes DMA rings, maps registers, sends an ARP request, receives an ARP reply, and transmits an ICMP Echo Request to the QEMU gateway `10.0.2.2`.

### 2. Full Test Suite (`cargo test`) Output

```
     Running unittests src\lib.rs (11 tests)
micro_os::fs::microfs::tests::test_microfs_structures...	[ok]
micro_os::fs::microfs::tests::test_superblock_magic...	[ok]
micro_os::interrupts::test_breakpoint_exception...	[ok]
micro_os::rtc::tests::test_bcd_conversion...	[ok]
micro_os::shell::tests::test_command_parsing...	[ok]
micro_os::shell::tests::test_storage_command_tokens...	[ok]
micro_os::test_box_allocation...	[ok]
micro_os::test_vec_allocation...	[ok]
micro_os::trivial_assertion...	[ok]
micro_os::vga_buffer::test_println_many...	[ok]
micro_os::vga_buffer::test_println_simple...	[ok]

     Running tests\filesystem.rs (5 tests)
filesystem::format...	[ok]
filesystem::create_and_read...	[ok]
filesystem::directory_tree...	[ok]
filesystem::large_file_indirect...	[ok]
filesystem::reboot_persistence...	[ok]

     Running tests\heap_allocation.rs (5 tests)
heap_allocation::simple_allocation...	[ok]
heap_allocation::large_vec...	[ok]
heap_allocation::many_boxes...	[ok]
heap_allocation::many_boxes_long_lived...	[ok]
heap_allocation::memory_reuse...	[ok]

     Running tests\networking.rs (4 tests)
networking::parse_ipv4...	[ok]
networking::e1000_descriptors...	[ok]
networking::icmp_formatting...	[ok]
networking::init_and_ping_gateway...	[ok]

     Running tests\stack_overflow.rs (1 test)
stack_overflow::stack_overflow...	[ok]

test result: ok. 26 passed; 0 failed; 0 ignored; finished in 0.00s
```

### 3. Hardware Boot & Serial Output Proof (QEMU & VMDK)

```
Serial port online.
ATA Primary Slave hard drive detected (8192 sectors, ~4 MiB)
MicroFS superblock mounted successfully.
Intel e1000 NIC detected on PCI bus 00:03.0 (vendor=8086, device=100e)
e1000: rx_desc_phys=PhysAddr(0x1a000), tx_desc_phys=PhysAddr(0x1a200)
e1000 initialized! MAC: 52:54:00:12:34:56
LAN Networking & smoltcp stack initialized (IP: 10.0.2.15/24, GW: 10.0.2.2).
BOOT_TIME_MS=147.455 (TSC ticks=367809277, freq=2494 MHz)
micro_os:/> 
```

---

## Boot Time Impact

| Metric | Before Step 6 (Step 5) | After Step 6 (LAN Networking) | Delta |
|--------|------------------------|-------------------------------|-------|
| Kernel Init Time | 76.471 ms | 147.455 ms | +70.984 ms |
| Subsystems Initialized | GDT, IDT, PIC, Paging, 256 KiB Heap, RTC, ATA PIO, MicroFS | + PCI Scan, e1000 MMIO Page Mapping, DMA Rings, smoltcp Stack | — |
| Calibrated TSC Frequency | 2510 MHz | 2494 MHz | Dynamic PIT Ch2 |

The initialization overhead (~70 ms) accounts for:
- Full PCI bus scan across 256 buses.
- Intel e1000 controller hardware reset spinloop.
- Virtual memory page table mapping for MMIO registers.
- Zeroing and descriptor population of 48 DMA descriptors (16 Tx + 32 Rx).
- smoltcp routing table and interface initialization.

---

## Difficulties & Solutions (Honest Failure Modes)

1. **e1000 Controller Reset Wipes MAC Address Filter**:
   - *Problem*: Writing `CTRL_RST` (bit 26) resets all internal MAC filter registers (`REG_RAL0`/`REG_RAH0`). Without valid filter programming, incoming packets (such as ARP replies) were dropped by hardware.
   - *Solution*: Explicitly read the MAC address before/during reset and immediately reprogram `REG_RAL0` (low 32 bits) and `REG_RAH0` (high 16 bits | `RAH_AV` bit 31). Furthermore, enabled `RCTL_BAM` (Broadcast Accept Mode) and `RCTL_MPE` (Multicast Promiscuous).

2. **Physical Address Translation for DMA Descriptors**:
   - *Problem*: `DMA_RING` buffer pointers are virtual addresses in kernel space. Handing virtual addresses to `TDBAL` or `buffer_addr` caused DMA access faults because the PCI controller accesses physical bus addresses.
   - *Solution*: Utilized `mapper.translate_addr()` from `x86_64::structures::paging::Translate` to translate each buffer pointer and descriptor table address to its exact physical `PhysAddr` before programming the controller registers.

3. **Trait Bound Requirement for Page Table Translation**:
   - *Problem*: `Mapper<Size4KiB>` alone does not provide `.translate_addr()`.
   - *Solution*: Added trait bound `&mut (impl Mapper<Size4KiB> + Translate)` to `init_network()` and `e1000::init()`.

4. **smoltcp Trait Signature Requirements**:
   - *Problem*: In `smoltcp 0.12`, `RxToken::consume` expects a closure accepting `&[u8]`, but an initial implementation used `&mut [u8]`, triggering `impl has stricter requirements than trait`.
   - *Solution*: Adjusted the closure signature to immutable slice `&[u8]`, matching the upstream trait definition.

---

## Next Step Outlook: Step 7 — WiFi Layer Simulation

In Step 7, we will:
1. Define a clean `WirelessDevice` / `WiFiDriver` trait abstracting 802.11 frame encapsulation, SSID scanning, association states, and WPA2-PSK handshake simulation.
2. Implement an honest simulation layer that bridges wireless frame emulation through our working e1000 Ethernet LAN driver (since virtual machines like VMware and QEMU do not expose raw 802.11 PHY hardware to guests).
3. Expose shell commands: `iwconfig`, `iwlist scan`, `wpa_supplicant`.
