pub mod e1000;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;
use spin::Mutex;
use x86_64::structures::paging::{FrameAllocator, Mapper, Size4KiB, Translate};

use smoltcp::iface::{Config, Interface, SocketSet};
use smoltcp::phy::{ChecksumCapabilities, Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::socket::{icmp, tcp};
use smoltcp::time::Instant;
use smoltcp::wire::{
    EthernetAddress, HardwareAddress, Icmpv4Packet, Icmpv4Repr, IpAddress, IpCidr, IpEndpoint,
    Ipv4Address, Ipv4Cidr,
};

use crate::interrupts::timer_ticks;
use crate::pci::find_e1000;
use crate::serial_println;
use e1000::E1000_DRIVER;

// Device wrapper for smoltcp
pub struct E1000Phy;

pub struct E1000RxToken {
    buffer: Vec<u8>,
}

pub struct E1000TxToken;

impl Device for E1000Phy {
    type RxToken<'a> = E1000RxToken where Self: 'a;
    type TxToken<'a> = E1000TxToken where Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let mut driver = E1000_DRIVER.lock();
        driver.receive().map(|buffer| (E1000RxToken { buffer }, E1000TxToken))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(E1000TxToken)
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1514;
        caps.medium = Medium::Ethernet;
        caps.checksum = ChecksumCapabilities::ignored();
        caps
    }
}

impl RxToken for E1000RxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.buffer)
    }
}

impl TxToken for E1000TxToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buf = vec![0u8; len];
        let res = f(&mut buf);
        E1000_DRIVER.lock().transmit(&buf);
        res
    }
}

pub struct NetworkStack {
    device: E1000Phy,
    interface: Interface,
    sockets: SocketSet<'static>,
    mac: [u8; 6],
    ip: Ipv4Address,
    gateway: Ipv4Address,
    prefix_len: u8,
}

impl NetworkStack {
    pub fn new(mac: [u8; 6]) -> Self {
        let mut device = E1000Phy;
        let eth_addr = EthernetAddress(mac);
        let config = Config::new(HardwareAddress::Ethernet(eth_addr));

        let now = Instant::from_millis(timer_ticks() as i64 * 10);
        let mut interface = Interface::new(config, &mut device, now);

        let default_ip = Ipv4Address::new(10, 0, 2, 15);
        let default_gw = Ipv4Address::new(10, 0, 2, 2);
        let prefix_len = 24;

        interface.update_ip_addrs(|addrs| {
            addrs
                .push(IpCidr::Ipv4(Ipv4Cidr::new(default_ip, prefix_len)))
                .ok();
        });
        interface.routes_mut().add_default_ipv4_route(default_gw).ok();

        let sockets = SocketSet::new(vec![]);

        NetworkStack {
            device,
            interface,
            sockets,
            mac,
            ip: default_ip,
            gateway: default_gw,
            prefix_len,
        }
    }

    pub fn poll(&mut self) {
        let now = Instant::from_millis(timer_ticks() as i64 * 10);
        self.interface
            .poll(now, &mut self.device, &mut self.sockets);
    }

    pub fn ifconfig(&self) -> String {
        let mut out = String::new();
        let (rx_pkts, rx_bytes, tx_pkts, tx_bytes) = E1000_DRIVER.lock().stats();
        let _ = write!(
            out,
            "eth0: flags=UP,BROADCAST,MULTICAST\n\
             \x20     ether {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\n\
             \x20     inet {}/{}  broadcast 10.0.2.255\n\
             \x20     gateway {}\n\
             \x20     RX packets {}  bytes {} ({:.1} KiB)\n\
             \x20     TX packets {}  bytes {} ({:.1} KiB)\n",
            self.mac[0],
            self.mac[1],
            self.mac[2],
            self.mac[3],
            self.mac[4],
            self.mac[5],
            self.ip,
            self.prefix_len,
            self.gateway,
            rx_pkts,
            rx_bytes,
            rx_bytes as f64 / 1024.0,
            tx_pkts,
            tx_bytes,
            tx_bytes as f64 / 1024.0
        );
        out
    }

    /// Ping target IP address via ICMP Echo Request and wait for Echo Reply.
    pub fn ping(&mut self, target: Ipv4Address, timeout_ms: u64) -> Result<u64, &'static str> {
        let rx_buffer = icmp::PacketBuffer::new(vec![icmp::PacketMetadata::EMPTY], vec![0u8; 256]);
        let tx_buffer = icmp::PacketBuffer::new(vec![icmp::PacketMetadata::EMPTY], vec![0u8; 256]);
        let mut icmp_socket = icmp::Socket::new(rx_buffer, tx_buffer);

        let ident = 0x4242;
        let seq = 1;
        icmp_socket.bind(icmp::Endpoint::Ident(ident)).map_err(|_| "Failed to bind ICMP socket")?;

        let socket_handle = self.sockets.add(icmp_socket);

        let payload = b"MicroOS_ICMP_Ping";
        let echo_repr = Icmpv4Repr::EchoRequest {
            ident,
            seq_no: seq,
            data: payload,
        };

        let mut icmp_packet = vec![0u8; echo_repr.buffer_len()];
        let mut packet = Icmpv4Packet::new_unchecked(&mut icmp_packet);
        echo_repr.emit(&mut packet, &ChecksumCapabilities::default());

        let start_ticks = timer_ticks();
        let mut last_send_ms = 0u64;
        let mut reply_received = false;
        let mut rtt_ms = 0;

        while (timer_ticks().saturating_sub(start_ticks) * 10) < timeout_ms {
            let elapsed_ms = timer_ticks().saturating_sub(start_ticks) * 10;
            let now = Instant::from_millis(timer_ticks() as i64 * 10);
            self.interface.poll(now, &mut self.device, &mut self.sockets);

            let socket = self.sockets.get_mut::<icmp::Socket>(socket_handle);

            // Transmit packet initially and re-transmit every 250ms if waiting on ARP
            if elapsed_ms >= last_send_ms && socket.can_send() {
                let _ = socket.send_slice(&icmp_packet, IpAddress::Ipv4(target));
                last_send_ms = elapsed_ms + 250;
            }

            if socket.can_recv() {
                let mut buf = [0u8; 256];
                if let Ok((len, _from)) = socket.recv_slice(&mut buf) {
                    if let Ok(reply_pkt) = Icmpv4Packet::new_checked(&buf[..len]) {
                        if reply_pkt.msg_type() == smoltcp::wire::Icmpv4Message::EchoReply {
                            reply_received = true;
                            rtt_ms = elapsed_ms.max(1);
                            break;
                        }
                    }
                }
            }
            core::hint::spin_loop();
        }

        self.sockets.remove(socket_handle);

        if reply_received {
            Ok(rtt_ms)
        } else {
            Err("Request timed out (no ICMP Echo Reply received)")
        }
    }

    /// Perform simple HTTP 1.0 GET request over TCP and return response string.
    pub fn http_get(
        &mut self,
        target: Ipv4Address,
        port: u16,
        path: &str,
        timeout_ms: u64,
    ) -> Result<String, &'static str> {
        let rx_buffer = tcp::SocketBuffer::new(vec![0u8; 4096]);
        let tx_buffer = tcp::SocketBuffer::new(vec![0u8; 4096]);
        let mut tcp_socket = tcp::Socket::new(rx_buffer, tx_buffer);

        let local_port = 49152 + ((timer_ticks() & 0x3FFF) as u16);
        let remote_endpoint = IpEndpoint::new(IpAddress::Ipv4(target), port);

        tcp_socket
            .connect(
                self.interface.context(),
                remote_endpoint,
                local_port,
            )
            .map_err(|_| "TCP connect failed")?;

        let socket_handle = self.sockets.add(tcp_socket);

        let start_ticks = timer_ticks();
        let mut request_sent = false;
        let mut response = String::new();
        let mut response_buf = [0u8; 1024];

        let mut req_str = String::new();
        let _ = write!(
            req_str,
            "GET {} HTTP/1.0\r\nHost: {}\r\nUser-Agent: MicroOS/0.6\r\nConnection: close\r\n\r\n",
            path, target
        );

        while (timer_ticks().saturating_sub(start_ticks) * 10) < timeout_ms {
            let now = Instant::from_millis(timer_ticks() as i64 * 10);
            self.interface.poll(now, &mut self.device, &mut self.sockets);

            let socket = self.sockets.get_mut::<tcp::Socket>(socket_handle);

            if socket.is_active() && socket.can_send() && !request_sent {
                if socket.send_slice(req_str.as_bytes()).is_ok() {
                    request_sent = true;
                }
            }

            if socket.can_recv() {
                if let Ok(bytes_read) = socket.recv_slice(&mut response_buf) {
                    if bytes_read > 0 {
                        if let Ok(chunk) = core::str::from_utf8(&response_buf[..bytes_read]) {
                            response.push_str(chunk);
                        }
                    }
                }
            }

            if request_sent && !socket.is_active() && !response.is_empty() {
                break;
            }

            core::hint::spin_loop();
        }

        self.sockets.remove(socket_handle);

        if !response.is_empty() {
            Ok(response)
        } else {
            Err("HTTP request timed out or received empty response")
        }
    }
}

pub static NETWORK: Mutex<Option<NetworkStack>> = Mutex::new(None);

/// Scan PCI for Intel e1000 and initialize networking subsystem.
pub fn init_network(
    mapper: &mut (impl Mapper<Size4KiB> + Translate),
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    if let Some(pci_dev) = find_e1000() {
        serial_println!(
            "Intel e1000 NIC detected on PCI bus {:02x}:{:02x}.{:x} (vendor={:04x}, device={:04x})",
            pci_dev.bus,
            pci_dev.device,
            pci_dev.function,
            pci_dev.vendor_id,
            pci_dev.device_id
        );

        let mut driver = E1000_DRIVER.lock();
        if let Err(e) = driver.init(&pci_dev, mapper, frame_allocator) {
            serial_println!("e1000 driver initialization error: {}", e);
            return;
        }

        let mac = driver.mac();
        serial_println!(
            "e1000 initialized! MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        );

        let stack = NetworkStack::new(mac);
        *NETWORK.lock() = Some(stack);
        serial_println!("LAN Networking & smoltcp stack initialized (IP: 10.0.2.15/24, GW: 10.0.2.2).");
    } else {
        serial_println!("No Intel e1000 NIC detected on PCI bus.");
    }
}
