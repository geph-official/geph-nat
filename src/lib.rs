use bytes::Bytes;
use parking_lot::Mutex;
use pnet_packet::ip::IpNextHeaderProtocols;
use pnet_packet::ipv4::MutableIpv4Packet;
use pnet_packet::tcp::MutableTcpPacket;
use pnet_packet::udp::MutableUdpPacket;
use pnet_packet::MutablePacket;
use std::net::{Ipv4Addr, SocketAddrV4};

mod bijective_lru;
pub use bijective_lru::BijectiveLru;

pub struct GephNat {
    map: Mutex<BijectiveLru<(u16, SocketAddrV4), (SocketAddrV4, SocketAddrV4)>>,
    src_ip: Ipv4Addr,
}

impl GephNat {
    pub fn new(cap: usize, src_ip: Ipv4Addr) -> Self {
        let map = Mutex::new(BijectiveLru::new(cap));
        Self { map, src_ip }
    }

    /// Given the source and destination addresses of an upstream packet, return the corresponding address on the "up-facing" side of the NAT
    pub fn rewrite_upstream_src(
        &self,
        original_src: SocketAddrV4,
        dest: SocketAddrV4,
    ) -> SocketAddrV4 {
        let existing = self.map.lock().get_key(&(original_src, dest)).cloned();
        if let Some((new_src_port, _)) = existing {
            self.map
                .lock()
                .push((new_src_port, dest), (original_src, dest));
            return SocketAddrV4::new(self.src_ip, new_src_port);
        }

        let new_src_port = if original_src.port() == 0 {
            0
        } else {
            loop {
                let candidate = fastrand::u16(2000..);
                if self.map.lock().get_value(&(candidate, dest)).is_none() {
                    break candidate;
                }
            }
        };
        self.map
            .lock()
            .push((new_src_port, dest), (original_src, dest));

        SocketAddrV4::new(self.src_ip, new_src_port)
    }

    /// Given the destination and source addresses of a downstream packet, return the corresponding address on the "down-facing" side of the NAT
    pub fn rewrite_downstream_dest(
        &self,
        current_dest: SocketAddrV4,
        src: SocketAddrV4,
    ) -> Option<SocketAddrV4> {
        let original = self
            .map
            .lock()
            .get_value(&(current_dest.port(), src))
            .cloned();
        original.map(|(x, _)| x)
    }

    /// Given an upstream IP packet, return the mangled version (if it parses successfully)
    pub fn mangle_upstream_pkt(&self, msg: &[u8]) -> Option<Bytes> {
        let mut bts = msg.to_vec();
        if let Some(mut ip_layer) = MutableIpv4Packet::new(&mut bts) {
            let src_ip = ip_layer.get_source();
            let dest_ip = ip_layer.get_destination();

            let next_level_protocol = ip_layer.get_next_level_protocol();
            if next_level_protocol == IpNextHeaderProtocols::Tcp {
                let mut tcp_layer = MutableTcpPacket::new(ip_layer.payload_mut())?;
                let src_port = tcp_layer.get_source();
                let dest_port = tcp_layer.get_destination();

                let new_src = self.rewrite_upstream_src(
                    SocketAddrV4::new(src_ip, src_port),
                    SocketAddrV4::new(dest_ip, dest_port),
                );

                tcp_layer.set_source(new_src.port());
                ip_layer.set_source(new_src.ip().to_owned());
            } else if next_level_protocol == IpNextHeaderProtocols::Udp {
                let mut udp_layer = MutableUdpPacket::new(ip_layer.payload_mut())?;
                let src_port = udp_layer.get_source();
                let dest_port = udp_layer.get_destination();

                let new_src = self.rewrite_upstream_src(
                    SocketAddrV4::new(src_ip, src_port),
                    SocketAddrV4::new(dest_ip, dest_port),
                );

                udp_layer.set_source(new_src.port());
                ip_layer.set_source(new_src.ip().to_owned());
            } else {
                log::debug!("original ICMP src IP: {:?}", src_ip);
                let fake = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0);
                // let dest = SocketAddrV4::new(dest_ip, 0);
                let new_src = self.rewrite_upstream_src(SocketAddrV4::new(src_ip, 0), fake);
                log::debug!("new ICMP src IP: {:?}", new_src.ip());
                ip_layer.set_source(new_src.ip().to_owned());
            };
            // fix all checksums
            fix_all_checksums(&mut ip_layer);
            Some(bts.into())
        } else {
            None
        }
    }

    /// Given a downstream IP packet, return the mangled form if it parses correctly.
    pub fn mangle_downstream_pkt(&self, msg: &[u8]) -> Option<Bytes> {
        let mut bts = msg.to_vec();
        if let Some(mut ip_layer) = MutableIpv4Packet::new(&mut bts) {
            let src_ip = ip_layer.get_source();
            let dest_ip = ip_layer.get_destination();

            let next_level_protocol = ip_layer.get_next_level_protocol();
            if next_level_protocol == IpNextHeaderProtocols::Tcp {
                let mut tcp_layer = MutableTcpPacket::new(ip_layer.payload_mut())?;
                let src_port = tcp_layer.get_source();
                let dest_port = tcp_layer.get_destination();

                let new_dest = self.rewrite_downstream_dest(
                    SocketAddrV4::new(dest_ip, dest_port),
                    SocketAddrV4::new(src_ip, src_port),
                )?;

                tcp_layer.set_destination(new_dest.port());
                ip_layer.set_destination(new_dest.ip().to_owned());
            } else if next_level_protocol == IpNextHeaderProtocols::Udp {
                let mut udp_layer = MutableUdpPacket::new(ip_layer.payload_mut())?;
                let src_port = udp_layer.get_source();
                let dest_port = udp_layer.get_destination();

                let new_dest = self.rewrite_downstream_dest(
                    SocketAddrV4::new(dest_ip, dest_port),
                    SocketAddrV4::new(src_ip, src_port),
                )?;

                udp_layer.set_destination(new_dest.port());
                ip_layer.set_destination(new_dest.ip().to_owned());
            } else {
                log::debug!("original ICMP dest IP: {:?}", dest_ip);
                let fake = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0);
                let new_dest = self.rewrite_downstream_dest(SocketAddrV4::new(dest_ip, 0), fake)?;

                log::debug!("new ICMP dest: {:?}", new_dest);
                ip_layer.set_destination(new_dest.ip().to_owned());
            };
            // fix all checksums
            fix_all_checksums(&mut ip_layer);
            Some(bts.into())
        } else {
            None
        }
    }
}

fn fix_all_checksums(ip_layer: &mut MutableIpv4Packet) -> Option<()> {
    let source = ip_layer.get_source();
    let dest = ip_layer.get_destination();

    if ip_layer.get_next_level_protocol() == IpNextHeaderProtocols::Udp {
        let mut udp_layer = MutableUdpPacket::new(ip_layer.payload_mut())?;
        let udp_checksum =
            pnet_packet::udp::ipv4_checksum(&udp_layer.to_immutable(), &source, &dest);
        udp_layer.set_checksum(udp_checksum);
    } else if ip_layer.get_next_level_protocol() == IpNextHeaderProtocols::Tcp {
        let mut tcp_layer = MutableTcpPacket::new(ip_layer.payload_mut())?;
        let tcp_checksum =
            pnet_packet::tcp::ipv4_checksum(&tcp_layer.to_immutable(), &source, &dest);
        tcp_layer.set_checksum(tcp_checksum);
    }
    let ip_checksum = pnet_packet::ipv4::checksum(&ip_layer.to_immutable());
    ip_layer.set_checksum(ip_checksum);
    Some(())
}
