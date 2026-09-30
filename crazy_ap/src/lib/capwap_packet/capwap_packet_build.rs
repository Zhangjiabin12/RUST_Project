use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt};
use crate::capwap_packet::message_element::element_wtp_board_data::{WtpBoardDataValue, WtpBoardData};
use crate::capwap_packet::message_element::element_discovery_type::DiscoveryType;
use crate::capwap_packet::message_element::element_wtp_descriptor::{WtpDescriptor,WtpDescriptorValue};
use crate::capwap_packet::message_element::element_wtp_frame_tunnel_mode::WtpFrameTunnelMode;
use crate::capwap_packet::message_element::element_wtp_mac_type::WtpMacType;
use crate::capwap_packet::message_element::element_ieee_80211_wtp_info::Ieee80211WtpInfo;
use crate::capwap_packet::tlv::{Tlv, VendorTlv, VendorSpciTlv};
use std::io::Cursor;

// //**ElementType**//
// const DiscoveryType: u16 = 20;
// const WtpBoardData: u16 = 38;
// const WtpDescriptor: u16 = 39;
// const WtpFrameTunnelMode: u16 = 41;
// const WtpMacType: u16 = 44;
// const Ieee80211WtpInfo: u16 = 1048;
// //**ElementType**//

const SSID_OPEN: &str = "00046f70656e";

#[derive(Debug, Clone)]
pub enum WtpEventType {
    StaIpReport,
    StaIpv6Report,
    ApInfoGatherReport,
}

pub struct CapwapPreamble {
    pub version: u8,
    pub type_: u8,
}

impl CapwapPreamble {
    // 将 CapwapPreamble 转换为字节序列
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let mut bytes = vec![];
        let preamble = (self.version << 4) | self.type_;
        bytes.write_u8(preamble)?;
        Ok(bytes)
    }
    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<Self> {
        let mut rdr = Cursor::new(bytes);
        let preamble = rdr.read_u8()?;
        Ok(Self {
            version: (preamble >> 4) & 0x0F,
            type_: preamble & 0x0F,
        })
    }
    pub fn get_data_length(&self) -> u8 {
        1
    }
}

pub struct CapwapHeader {
    pub header_length: u16,
    pub radio_id: u16,
    pub wireless_binding_id: u16,
    pub payload_type: u16,
    pub fragment: u8,
    pub last_fragment: u8,
    pub wireless_header: u8,
    pub radio_mac_header: u8,
    pub keep_alive: u8,
    pub reserved: u8,
    pub fragment_id: u16,
    pub fragment_offset: u16,
    pub reserved2: u16,
}

impl CapwapHeader {
    // 将 CapwapHeader 转换为字节序列
    fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let mut bytes = vec![];
        let header = (self.header_length << 11) | (self.radio_id << 6) | (self.wireless_binding_id << 1) |
                     (self.payload_type << 0);
        let header_u8 = (self.fragment << 7) | (self.last_fragment << 6) |
        (self.wireless_header << 5) | (self.radio_mac_header << 4) | (self.keep_alive << 3) | self.reserved;
        bytes.write_u16::<BigEndian>(header)?;
        bytes.write_u8(header_u8)?;
        
        bytes.write_u16::<BigEndian>(self.fragment_id)?;
        let fragment = (self.fragment_offset << 3) | self.reserved2;
        bytes.write_u16::<BigEndian>(fragment)?;

        Ok(bytes)
    }
    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<Self> {
        let mut rdr = Cursor::new(bytes);

        let header = rdr.read_u16::<BigEndian>()?;
        let header_length = (header >> 11) & 0x1F;
        let radio_id = (header >> 6) & 0x1F;
        let wireless_binding_id = (header >> 1) & 0x1F;
        let payload_type = header & 0x01F;

        let header_u8 = rdr.read_u8()?;
        let fragment = (header_u8 >> 7) & 0x01;
        let last_fragment = (header_u8 >> 6) & 0x01;
        let wireless_header = (header_u8 >> 5) & 0x01;
        let radio_mac_header = (header_u8 >> 4) & 0x01;
        let keep_alive = (header_u8 >> 3) & 0x01;
        let reserved = header_u8 & 0x07;

        let fragment_id = rdr.read_u16::<BigEndian>()?;

        let fragment2 = rdr.read_u16::<BigEndian>()?;
        let fragment_offset = (fragment2 >> 3) & 0x1FFF;
        let reserved2 = fragment2 & 0xff;

        Ok(Self {
            header_length,
            radio_id,
            wireless_binding_id,
            payload_type,
            fragment,
            last_fragment,
            wireless_header,
            radio_mac_header,
            keep_alive,
            reserved,
            fragment_id,
            fragment_offset,
            reserved2,
        })
    }
    pub fn get_data_length(&self) -> u8 {
        7
    }
}

pub struct CapwapControlHeader {
    pub message_type_reserved: u32,
    pub message_type: u32,
    pub sequence_number: u8,
    pub message_length: u16,
    pub flags: u8,
}

impl CapwapControlHeader {
    // 将 CapwapControlHeader 转换为字节序列
    fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let mut bytes = vec![];
        let message_type = (self.message_type_reserved << 8) | self.message_type;
        bytes.write_u32::<BigEndian>(message_type)?;
        bytes.write_u8(self.sequence_number)?;
        bytes.write_u16::<BigEndian>(self.message_length)?;
        bytes.write_u8(self.flags)?;
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<Self> {
        let mut rdr = Cursor::new(bytes);
        let message_type = rdr.read_u32::<BigEndian>()?;
        let message_type_reserved = (message_type >> 8) & 0xffff;
        let message_type = message_type & 0x7F;
        let sequence_number = rdr.read_u8()?;
        let message_length = rdr.read_u16::<BigEndian>()?;
        let flags = rdr.read_u8()?;
        Ok(Self {
            message_type_reserved,
            message_type,
            sequence_number,
            message_length,
            flags,
        })
    }
    pub fn get_data_length(&self) -> u8 {
        8
    }
    
}




//** CAPWAP_DISCOVERY **//
pub fn crate_discovery(ap_mac: &str, ap_serial: &str) -> std::io::Result<Vec<u8>> {
    let preamble = CapwapPreamble {
        version: 0,
        type_: 0,
    };
    let header = CapwapHeader {
        header_length: 2,
        radio_id: 1,
        wireless_binding_id: 1,
        payload_type: 0,
        fragment: 0,
        last_fragment: 0,
        wireless_header: 0,
        radio_mac_header: 0,
        keep_alive: 0,
        reserved: 0,
        fragment_id: 0,
        fragment_offset: 0,
        reserved2: 0,
    };
    
    let control_header = CapwapControlHeader {
        message_type_reserved: 0,
        message_type: 1,
        sequence_number: 0,
        message_length: 191,
        flags: 0,
    };

    let discovery_type = DiscoveryType {
        type_: 20,
        length: 1,
        data: 1,
    };

    let wtp_board_data = WtpBoardData {
        type_: 38,
        length: 52,
        wtp_board_data_vendor: 5651,
        data_modle_number: WtpBoardDataValue::WtpModelNumber(Tlv {
            type_: 0,
            length: 18,
            value: hex::decode("5741323630302d3832322d50452856313129").unwrap(),
        }),
        data_serial_number: WtpBoardDataValue::WtpSerialNumber(Tlv {
            type_: 1,
            length: (ap_serial.len() / 2) as u16,
            value: hex::decode(ap_serial).unwrap(),
        }),
        data_board_revision: WtpBoardDataValue::BoardRevision(Tlv {
            type_: 3,
            length: 1,
            value: hex::decode("38").unwrap(),
        }),
        data_base_mac_address: WtpBoardDataValue::BaseMacAddress(Tlv {
            type_: 4,
            length: 6,
            value: hex::decode(ap_mac).unwrap(),
        }),

    }; 

    let wtp_descriptor = WtpDescriptor {
        type_: 39,
        length: 95,
        max_radius: 2,
        radio_in_use: 2,
        encryption_capabilities_number: 1,
        encryption_capabilities_encrypt:3,
        encryption_capabilities_encrypt_wbid:1,
        encryption_capabilities:12,
        wtp_descriptor_vendor:5651,
        wtp_hardware_version: WtpDescriptorValue::WtpHardwareVersion(Tlv {
            type_:0,
            length:4,
            value:hex::decode("56312e31").unwrap(),
        }),
        wtp_active_software_version:WtpDescriptorValue::WtpActiveSoftwareVersion(VendorTlv {
            wtp_description: 5651,
            type_:1,
            length:18,
            value:hex::decode("563330305230303643313042303037285229").unwrap(),
        }),
        wtp_boot_version:WtpDescriptorValue::WtpBootVersion(VendorTlv {
            wtp_description:5651,
            type_:2,
            length:6,
            value:hex::decode("76312e302e33").unwrap(),
        }),
        wtp_other_softer_version:WtpDescriptorValue::WtpOtherSofterVersion(VendorTlv {
            wtp_description:5651,
            type_:3,
            length:16,
            value:hex::decode("56333030523030354330312e42303033").unwrap(),
        }),
        unknown_wtp_descriptor:WtpDescriptorValue::UnknownWtpDescriptor(VendorTlv {
            wtp_description:5651,
            type_:4,
            length:5,
            value:hex::decode("4d61697075").unwrap(),
        }),
    };
            
    let wtp_frame_tunnel_mod = WtpFrameTunnelMode {
        type_: 41,
        length: 1,
        value: 8,
    };

    let mac_type = WtpMacType {
        mac_type: 44,
        length: 1,
        value: 2,
    };

    let radio1_info = Ieee80211WtpInfo{
        type_:1048,
        length:5,
        value:hex::decode("010000002d").unwrap(),
    };

    let radio2_info = Ieee80211WtpInfo{
        type_:1048,
        length:5,
        value:hex::decode("020000003a").unwrap(),
    };

    let mut discovery_bytes = preamble.to_bytes()?;
    discovery_bytes.append(&mut header.to_bytes()?);
    discovery_bytes.append(&mut control_header.to_bytes()?);
    discovery_bytes.append(&mut discovery_type.to_bytes()?);
    discovery_bytes.append(&mut wtp_board_data.to_bytes()?);
    discovery_bytes.append(&mut wtp_descriptor.to_bytes()?);
    discovery_bytes.append(&mut wtp_frame_tunnel_mod.to_bytes()?);
    discovery_bytes.append(&mut mac_type.to_bytes()?);
    discovery_bytes.append(&mut radio1_info.to_bytes()?);
    discovery_bytes.append(&mut radio2_info.to_bytes()?);
    Ok(discovery_bytes)


}
//** CAPWAP_DISCOVERY **//

//** CAPWAP_JOIN **//
pub fn crate_join(ap_mac: &str, ap_ip: &str, ap_serial: &str) -> std::io::Result<Vec<u8>> {
    let preamble = CapwapPreamble {
        version: 0,
        type_: 0,
    };
    let header = CapwapHeader {
        header_length: 2,
        radio_id: 1,
        wireless_binding_id: 1,
        payload_type: 0,
        fragment: 0,
        last_fragment: 0,
        wireless_header: 0,
        radio_mac_header: 0,
        keep_alive: 0,
        reserved: 0,
        fragment_id: 0,
        fragment_offset: 0,
        reserved2: 0,
    };
    
    let control_header = CapwapControlHeader {
        message_type_reserved: 0,
        message_type: 3,
        sequence_number: 1,
        message_length: 191,
        flags: 0,
    };

    let location_data = Tlv {
        type_: 28,
        length: 0,
        value: hex::decode("").unwrap(),
    };

    let wtp_board_data = WtpBoardData {
        type_: 38,
        length: 52,
        wtp_board_data_vendor: 5651,
        data_modle_number: WtpBoardDataValue::WtpModelNumber(Tlv {
            type_: 0,
            length: 18,
            value: hex::decode("5741323630302d3832322d50452856313129").unwrap(),
        }),
        data_serial_number: WtpBoardDataValue::WtpSerialNumber(Tlv {
            type_: 1,
            length: (ap_serial.len() / 2) as u16,
            value: hex::decode(ap_serial).unwrap(),
        }),
        data_board_revision: WtpBoardDataValue::BoardRevision(Tlv {
            type_: 3,
            length: 1,
            value: hex::decode("38").unwrap(),
        }),
        data_base_mac_address: WtpBoardDataValue::BaseMacAddress(Tlv {
            type_: 4,
            length: 6,
            value: hex::decode(ap_mac).unwrap(),
        }),

    }; 

    let wtp_descriptor = WtpDescriptor {
        type_: 39,
        length: 95,
        max_radius: 2,
        radio_in_use: 2,
        encryption_capabilities_number: 1,
        encryption_capabilities_encrypt:3,
        encryption_capabilities_encrypt_wbid:1,
        encryption_capabilities:12,
        wtp_descriptor_vendor:5651,
        wtp_hardware_version: WtpDescriptorValue::WtpHardwareVersion(Tlv {
            type_:0,
            length:4,
            value:hex::decode("56312e31").unwrap(),
        }),
        wtp_active_software_version:WtpDescriptorValue::WtpActiveSoftwareVersion(VendorTlv {
            wtp_description: 5651,
            type_:1,
            length:18,
            value:hex::decode("563330305230303643313042303037285229").unwrap(),
        }),
        wtp_boot_version:WtpDescriptorValue::WtpBootVersion(VendorTlv {
            wtp_description:5651,
            type_:2,
            length:6,
            value:hex::decode("76312e302e33").unwrap(),
        }),
        wtp_other_softer_version:WtpDescriptorValue::WtpOtherSofterVersion(VendorTlv {
            wtp_description:5651,
            type_:3,
            length:16,
            value:hex::decode("56333030523030354330312e42303033").unwrap(),
        }),
        unknown_wtp_descriptor:WtpDescriptorValue::UnknownWtpDescriptor(VendorTlv {
            wtp_description:5651,
            type_:4,
            length:5,
            value:hex::decode("4d61697075").unwrap(),
        }),
    };

    let wtp_name = Tlv {
        type_: 45,
        length: 18,
        value: hex::decode("5741323630302d3832322d50452856313129").unwrap(),
    };

    let session_id = Tlv {
        type_: 35,
        length: (format!("{}8fdc7336600cf44333d2",ap_mac).len()/2) as u16,
        value: hex::decode(format!("{}8fdc7336600cf44333d2",ap_mac)).unwrap(),
    };

    let wtp_frame_tunnel_mod = WtpFrameTunnelMode {
        type_: 41,
        length: 1,
        value: 8,
    };

    let mac_type = WtpMacType {
        mac_type: 44,
        length: 1,
        value: 2,
    };

    let radio1_info = Ieee80211WtpInfo{
        type_:1048,
        length:5,
        value:hex::decode("010000002d").unwrap(),
    };

    let radio2_info = Ieee80211WtpInfo{
        type_:1048,
        length:5,
        value:hex::decode("020000003a").unwrap(),
    };

    let ap_alarm = VendorSpciTlv {
        type_: 37,
        length: ("0000".len()/2 + 6) as u16,  // vendor id + vendor element id = 6(len)
        vendor_id: 5651,
        vendor_element_id: 83,
        value: hex::decode("0000").unwrap(),
    };

    let ap_software_name = VendorSpciTlv {
        type_: 37,
        length: ("00144150204d616e6167656d656e742053797374656d".len()/2 + 6) as u16,
        vendor_id: 5651,
        vendor_element_id: 47,
        value: hex::decode("00144150204d616e6167656d656e742053797374656d").unwrap(),
    };

    let capwap_local_ipv4_address = Tlv {
        type_: 30,
        length: ("7f000001".len()/2 ) as u16,
        value: hex::decode(ap_ip).unwrap(),
    };

    let ap_system_netmask = VendorSpciTlv {
        type_: 37,
        length: ("0004ffffff00".len()/2 + 6) as u16,
        vendor_id: 5651,
        vendor_element_id: 48,
        value: hex::decode("0004ffffff00").unwrap(),
    };
    let mut join_bytes = preamble.to_bytes()?;
    join_bytes.append(&mut header.to_bytes()?);
    join_bytes.append(&mut control_header.to_bytes()?);
    join_bytes.append(&mut location_data.to_bytes()?);
    join_bytes.append(&mut wtp_board_data.to_bytes()?);
    join_bytes.append(&mut wtp_descriptor.to_bytes()?);
    join_bytes.append(&mut wtp_name.to_bytes()?);
    join_bytes.append(&mut session_id.to_bytes()?);
    join_bytes.append(&mut wtp_frame_tunnel_mod.to_bytes()?);
    join_bytes.append(&mut mac_type.to_bytes()?);
    join_bytes.append(&mut radio1_info.to_bytes()?);
    join_bytes.append(&mut radio2_info.to_bytes()?);
    join_bytes.append(&mut ap_alarm.to_bytes()?);
    join_bytes.append(&mut ap_software_name.to_bytes()?);
    join_bytes.append(&mut capwap_local_ipv4_address.to_bytes()?);
    join_bytes.append(&mut ap_system_netmask.to_bytes()?);
    Ok(join_bytes)


}
//** CAPWAP_JOIN **//

//** CAPWAP_CONFIGURATION_STATUS_REQUEST **//
pub fn crate_configuration_status_request(ap_radio1_mac: &str, ap_radio2_mac: &str) -> std::io::Result<Vec<u8>> {
    
    // message_element //
    let radio_admin_state1 = Tlv {
        type_: 31,
        length: ("0101".len()/2) as u16,
        value: hex::decode("0101").unwrap(),
    };

    let radio_admin_state2 = Tlv {
        type_: 31,
        length: ("0201".len()/2) as u16,
        value: hex::decode("0201").unwrap(),
    };
    let ap_support_feature = VendorSpciTlv {
        type_: 37,
        length: ("ffdf1e00".len()/2 + 6) as u16,  // vendor id + vendor element id = 6(len)
        vendor_id: 5651,
        vendor_element_id: 90,
        value: hex::decode("ffdf1e00").unwrap(),
    };
    

    let wtp_reboot_statistics = Tlv {
        type_: 48,
        length: ("000000000000000000000000000000".len()/2) as u16,
        value: hex::decode("000000000000000000000000000000").unwrap(),
    };

    let ieee80211_direct_sequence_control = Tlv {
        type_: 1028,
        length: ("0100000000000000".len()/2) as u16,
        value: hex::decode("0100000000000000").unwrap(),
    };

    let ieee80211_ofdm_control = Tlv {
        type_: 1033,
        length: ("0200000f00000000".len()/2) as u16,
        value: hex::decode("0200000f00000000").unwrap(),
    };
    
    let ieee80211_support_rates1 = Tlv {
        type_: 1040,
        length: ("010000000000000000".len()/2) as u16,
        value: hex::decode("010000000000000000").unwrap(),
    };
    
    let ieee80211_support_rates2 = Tlv {
        type_: 1040,
        length: ("020000000000000000".len()/2) as u16,
        value: hex::decode("020000000000000000").unwrap(),
    };

    let ieee80211_wtp_radio_config1 = Tlv {
        type_: 1046,
        length: (format!("01001003{}0064434e0000",ap_radio1_mac).len()/2)as u16,
        value: hex::decode(format!("01001003{}0064434e0000",ap_radio1_mac)).unwrap(),
    };
    // format!("01001003{}064434e0000",ap_radio1_mac)

    let ieee80211_wtp_radio_config2 = Tlv {
        type_: 1046,
        length: (format!("02001003{}0064434e0000",ap_radio2_mac).len()/2)as u16,
        value: hex::decode(format!("02001003{}0064434e0000",ap_radio2_mac)).unwrap(),
    };
    
    let ieee80211_tx_power1 = Tlv {
        type_: 1041,
        length: ("0100001a".len()/2) as u16,
        value: hex::decode("0100001a").unwrap(),
    };

    let ieee80211_tx_power2 = Tlv {
        type_: 1041,
        length: ("0200001a".len()/2) as u16,
        value: hex::decode("0200001a").unwrap(),
    };

    let ieee80211_wtp_radio_info1 = Tlv {
        type_: 1048,
        length: ("010000002d".len() /2) as u16,
        value: hex::decode("010000002d").unwrap(),
    };

    let ieee80211_wtp_radio_info2 = Tlv {
        type_: 1048,
        length: ("020000003a".len() /2) as u16,
        value: hex::decode("020000003a").unwrap(),
    };

    let mut last_bytes = hex::decode("002500080000161300f60180002500080000161300f602700025000e00001613001e00060120030301010025000e00001613001e00060220030301010025000c00001613001f00040080010000250007000016130012010025000d0000161300260201031a02031a0025000800001613001700010025000a0000161300200002000100250007000016130038000025001400001613003901020104657468300204657468310025000a0000161300d6000100030025000800001613004300c000250007000016130094010025000a0000161300cd000100000025000d0000161300db00010101010003002500ab0000161300f8000000a101020000002f434e3d6d61696161732e746f7000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000063e5890065c80dff000000100152b03916b2c7e97eb5750941fb392a0004000c434a322d313030302d414332002400020000").unwrap();


    let control_header = CapwapControlHeader {
        message_type_reserved: 0,
        message_type: 5,
        sequence_number: 2,
        message_length: 602,
        flags: 0,
    };

    let header = CapwapHeader {
        header_length: 2,
        radio_id: 1,
        wireless_binding_id: 1,
        payload_type: 0,
        fragment: 0,
        last_fragment: 0,
        wireless_header: 0,
        radio_mac_header: 0,
        keep_alive: 0,
        reserved: 0,
        fragment_id: 0,
        fragment_offset: 0,
        reserved2: 0,
    };

    let preamble = CapwapPreamble {
        version: 0,
        type_: 0,
    };

    let mut config_status_request_bytes = preamble.to_bytes()?;
    config_status_request_bytes.append(&mut header.to_bytes()?);
    config_status_request_bytes.append(&mut control_header.to_bytes()?);
    config_status_request_bytes.append(&mut radio_admin_state1.to_bytes()?);
    config_status_request_bytes.append(&mut radio_admin_state2.to_bytes()?);
    config_status_request_bytes.append(&mut ap_support_feature.to_bytes()?);
    config_status_request_bytes.append(&mut wtp_reboot_statistics.to_bytes()?);
    config_status_request_bytes.append(&mut ieee80211_direct_sequence_control.to_bytes()?);
    config_status_request_bytes.append(&mut ieee80211_ofdm_control.to_bytes()?);
    config_status_request_bytes.append(&mut ieee80211_support_rates1.to_bytes()?);
    config_status_request_bytes.append(&mut ieee80211_support_rates2.to_bytes()?);
    config_status_request_bytes.append(&mut ieee80211_wtp_radio_config1.to_bytes()?);
    config_status_request_bytes.append(&mut ieee80211_wtp_radio_config2.to_bytes()?);
    config_status_request_bytes.append(&mut ieee80211_tx_power1.to_bytes()?);
    config_status_request_bytes.append(&mut ieee80211_tx_power2.to_bytes()?);
    config_status_request_bytes.append(&mut ieee80211_wtp_radio_info1.to_bytes()?);
    config_status_request_bytes.append(&mut ieee80211_wtp_radio_info2.to_bytes()?);
    config_status_request_bytes.append(&mut last_bytes);
    Ok(config_status_request_bytes)
}
//** CAPWAP_CONFIGURATION_STATUS_REQUEST **//

//** CAPWAP_CHANGE_STATE_REQUEST **//
pub fn crate_change_state_request() -> std::io::Result<Vec<u8>> {
    let radio_operational_state1 = Tlv {
        type_: 32,
        length:("010100".len()/2) as u16,
        value: hex::decode("010100").unwrap(),
    };

    let radio_operational_state2 = Tlv {
        type_: 32,
        length: ("020100".len()/2) as u16,
        value: hex::decode("020100").unwrap(),
    };

    let result_code = Tlv {
        type_: 33,
        length: ("00000000".len()/2) as u16,
        value: hex::decode("00000000").unwrap(),
    };

    let control_header = CapwapControlHeader {
        message_type_reserved: 0,
        message_type: 11,
        sequence_number: 3,
        message_length: result_code.length as u16 + radio_operational_state1.length as u16 + radio_operational_state2.length as u16,
        flags: 0,
    };

    let header = CapwapHeader {
        header_length: 2,
        radio_id: 1,
        wireless_binding_id: 1,
        payload_type: 0,
        fragment: 0,
        last_fragment: 0,
        wireless_header: 0,
        radio_mac_header: 0,
        keep_alive: 0,
        reserved: 0,
        fragment_id: 0,
        fragment_offset: 0,
        reserved2: 0,
    };

    let preamble = CapwapPreamble {
        version: 0,
        type_: 0,
    };

    let mut change_state_request_bytes = preamble.to_bytes()?;
    change_state_request_bytes.append(&mut header.to_bytes()?);
    change_state_request_bytes.append(&mut control_header.to_bytes()?);
    change_state_request_bytes.append(&mut radio_operational_state1.to_bytes()?);
    change_state_request_bytes.append(&mut radio_operational_state2.to_bytes()?);
    change_state_request_bytes.append(&mut result_code.to_bytes()?);
    Ok(change_state_request_bytes)
}
//** CAPWAP_CHANGE_STATE_REQUEST **//

//** KEEPALIVE **//
pub fn crate_keepalive(ap_mac: &str) -> std::io::Result<Vec<u8>> {
    
    let mut keep_alive = hex::decode(format!("001600230010{}8fdc7336600cf44333d2",ap_mac)).unwrap();
    
    let header = CapwapHeader {
        header_length: 2,
        radio_id: 0,
        wireless_binding_id: 1,
        payload_type: 0,
        fragment: 0,
        last_fragment: 0,
        wireless_header: 0,
        radio_mac_header: 0,
        keep_alive: 1,
        reserved: 0,
        fragment_id: 0,
        fragment_offset: 0,
        reserved2: 0,
    };

    let preamble = CapwapPreamble {
        version: 0,
        type_: 0,
    };

    let mut keep_alive_bytes = preamble.to_bytes()?;
    keep_alive_bytes.append(&mut header.to_bytes()?);
    keep_alive_bytes.append(&mut keep_alive);
    Ok(keep_alive_bytes)
}
//** KEEPALIVE **/

//**CONFIGURATION_UPDATE_RESPONSE */
pub fn crate_configuration_update_response(seq_num: u8) -> std::io::Result<Vec<u8>> {

    let result_code = Tlv {
        type_: 33,
        length  : ("00000000".len()/2) as u16,
        value: hex::decode("00000000").unwrap(),
    };

    let control_header = CapwapControlHeader {
        message_type_reserved: 0,
        message_type: 8,
        sequence_number: seq_num,
        message_length: 11,
        flags: 0,
    };

    let header = CapwapHeader {
        header_length: 2,
        radio_id: 1,
        wireless_binding_id: 1,
        payload_type: 0,
        fragment: 0,
        last_fragment: 0,
        wireless_header: 0,
        radio_mac_header: 0,
        keep_alive: 0,
        reserved: 0,
        fragment_id: 0,
        fragment_offset: 0,
        reserved2: 0,
    };

    let preamble = CapwapPreamble {
        version: 0,
        type_: 0,
    };

    let mut configuration_update_response_bytes = preamble.to_bytes()?;
    configuration_update_response_bytes.append(&mut header.to_bytes()?);
    configuration_update_response_bytes.append(&mut control_header.to_bytes()?);
    configuration_update_response_bytes.append(&mut result_code.to_bytes()?);
    Ok(configuration_update_response_bytes)
}
//**CONFIGURATION_UPDATE_RESPONSE */

//** IEEE80211_CONFIG_RESPONSE **/
pub fn crate_80211_config_response(ap_radio1_mac: &str, ap_radio2_mac: &str, seq_num: u8) -> std::io::Result<Vec<u8>> {
    


    let ieee8211_assigned_wtp_bssid2 = Tlv {
        type_: 1026,
        length  : (format!("0201{}",ap_radio2_mac).len()/2) as u16,
        value: hex::decode(format!("0201{}",ap_radio2_mac)).unwrap(),
    };

        
    let ieee8211_assigned_wtp_bssid1 = Tlv {
        type_: 1026,
        length  : (format!("0101{}",ap_radio1_mac).len()/2) as u16,
        value: hex::decode(format!("0101{}",ap_radio1_mac)).unwrap(),
    };

    let result_code = Tlv {
        type_: 33,
        length  : ("00000000".len()/2) as u16,
        value: hex::decode("00000000").unwrap(),
    };

    let control_header = CapwapControlHeader {
        message_type_reserved: 0,
        message_type: 3398914,
        sequence_number: seq_num,
        message_length: 35,
        flags: 0,
    };

    let header = CapwapHeader {
        header_length: 2,
        radio_id: 1,
        wireless_binding_id: 1,
        payload_type: 0,
        fragment: 0,
        last_fragment: 0,
        wireless_header: 0,
        radio_mac_header: 0,
        keep_alive: 0,
        reserved: 0,
        fragment_id: 0,
        fragment_offset: 0,
        reserved2: 0,
    };

    let preamble = CapwapPreamble {
        version: 0,
        type_: 0,
    };

    let mut ieee80211_config_response_bytes = preamble.to_bytes()?;
    ieee80211_config_response_bytes.append(&mut header.to_bytes()?);
    ieee80211_config_response_bytes.append(&mut control_header.to_bytes()?);
    ieee80211_config_response_bytes.append(&mut result_code.to_bytes()?);
    ieee80211_config_response_bytes.append(&mut ieee8211_assigned_wtp_bssid1.to_bytes()?);
    ieee80211_config_response_bytes.append(&mut ieee8211_assigned_wtp_bssid2.to_bytes()?);
    Ok(ieee80211_config_response_bytes)
    
}
//** IEEE80211_CONFIG_RESPONSE **/

// ** ECHO_REQUEST **/
pub fn crate_echo_request(ap_mac : &str) -> std::io::Result<Vec<u8>> {
    let wtp_session_id = VendorSpciTlv {
        type_: 37,
        length  : (format!("{}8fdc7336600cf44333d2",ap_mac).len()/2 + 6) as u16,
        vendor_id: 5651,
        vendor_element_id: 101,
        value: hex::decode(format!("{}8fdc7336600cf44333d2",ap_mac)).unwrap(),
    };


    let control_header = CapwapControlHeader {
        message_type_reserved: 0,
        message_type: 13,
        sequence_number: 0,
        message_length: 29,
        flags: 0,
    };

    let header = CapwapHeader {
        header_length: 2,
        radio_id: 0,
        wireless_binding_id: 1,
        payload_type: 0,
        fragment: 0,
        last_fragment: 0,
        wireless_header: 0,
        radio_mac_header: 0,
        keep_alive: 0,
        reserved: 0,
        fragment_id: 0,
        fragment_offset: 0,
        reserved2: 0,
    };

    let preamble = CapwapPreamble {
        version: 0,
        type_: 0,
    };

    let mut echo_request_bytes = preamble.to_bytes()?;
    echo_request_bytes.append(&mut header.to_bytes()?);
    echo_request_bytes.append(&mut control_header.to_bytes()?);
    echo_request_bytes.append(&mut wtp_session_id.to_bytes()?);
    Ok(echo_request_bytes)
}
// ** ECHO_REQUEST **// 


//** Association Request **//
pub fn crate_association_request(ap_radio_mac: &str, sta_mac: &str, am_radio_mac: &str) -> std::io::Result<Vec<u8>> {
    let mut any_bytes = hex::decode(format!("{}010882848b0c12961824c70110210208132d1aef0913ffff00000000000000000000000000000000000000010032043048606c3b10515153547374757677787c7d7e7f8082460573109100047f0a04000a02000000400020bf0c92f18033faff6203faff6223dd070050f202000100dd0f8cfdf0010102010002010109020300ff1a2303091082400000304c090dc08308000c00fafffaff191cc771",SSID_OPEN)).unwrap();
    let mut fix_parameters = hex::decode("01140100").unwrap();
    let mut association_request = hex::decode(format!("00003a01{}{}{}8098",ap_radio_mac, sta_mac, am_radio_mac)).unwrap();
    let mut bytes = Vec::new();
    bytes.append(&mut association_request);
    bytes.append(&mut fix_parameters);
    bytes.append(&mut any_bytes);
    Ok(bytes)

}

//** Association Request **//

//** Any tunnel messafe **//
pub fn crate_tunnel_message(mut message: Vec<u8>, ap_radio_mac: &str) -> std::io::Result<Vec<u8>> {
    let header = CapwapHeader {
        header_length: 6,
        radio_id: 1,
        wireless_binding_id: 1,
        payload_type: 1,
        fragment: 0,
        last_fragment: 0,
        wireless_header: 1,
        radio_mac_header: 1,
        keep_alive: 0,
        reserved: 0,
        fragment_id: 0,
        fragment_offset: 0,
        reserved2: 0,
    };

    let preamble = CapwapPreamble {
        version: 0,
        type_: 0,
    };

    let mut wireless_header = hex::decode(format!("06{}000400000000000000",ap_radio_mac)).unwrap();
    let mut bytes = preamble.to_bytes()?;
    bytes.append(&mut header.to_bytes()?);
    bytes.append(&mut wireless_header);
    bytes.append(&mut message);
    Ok(bytes)
}
//** Any tunnel messafe **//

//** WTP_EVENT_REPORT **//
pub fn crate_wtp_event_request(sta_mac: &str,sta_ip:&str ,message_type: WtpEventType) -> std::io::Result<Vec<u8>> {

    let header = CapwapHeader {
        header_length: 2,
        radio_id: 1,
        wireless_binding_id: 1,
        payload_type: 0,
        fragment: 0,
        last_fragment: 0,
        wireless_header: 0,
        radio_mac_header: 0,
        keep_alive: 0,
        reserved: 0,
        fragment_id: 0,
        fragment_offset: 0,
        reserved2: 0,
    };

    let preamble = CapwapPreamble {
        version: 0,
        type_: 0,
    };

    match message_type {
        WtpEventType::ApInfoGatherReport => {
            let control_header = CapwapControlHeader {
                message_type_reserved: 0,
                message_type: 9,
                sequence_number: 17,
                message_length: 29,
                flags: 0,
            };

            let ap_info_gather = VendorSpciTlv {
                type_:37,
                length: 22,
                vendor_id: 5651,
                vendor_element_id: 108,
                value: hex::decode(format!("000e0007000a{}00000036", sta_mac)).unwrap(),
            };
            let mut wtp_event_bytes = preamble.to_bytes()?;
            wtp_event_bytes.append(&mut header.to_bytes()?);
            wtp_event_bytes.append(&mut control_header.to_bytes()?);
            wtp_event_bytes.append(&mut ap_info_gather.to_bytes()?);
            Ok(wtp_event_bytes)
        }
        WtpEventType::StaIpReport => {

            let control_header = CapwapControlHeader {
                message_type_reserved: 0,
                message_type: 9,
                sequence_number: 18,
                message_length: 28,
                flags: 0,
            };

            let sta_ipv4_report = VendorSpciTlv {
                type_:37,
                length:21,
                vendor_id:5651,
                vendor_element_id: 85,
                value: hex::decode(format!("0101000b01{}{}",sta_mac, sta_ip)).unwrap(),
            };
            let mut wtp_event_bytes = preamble.to_bytes()?;
            wtp_event_bytes.append(&mut header.to_bytes()?);
            wtp_event_bytes.append(&mut control_header.to_bytes()?);
            wtp_event_bytes.append(&mut sta_ipv4_report.to_bytes()?);
            Ok(wtp_event_bytes)
        }
        WtpEventType::StaIpv6Report => {
            Ok(Vec::new())
        }
    }
    

}
//** WTP_EVENT_REPORT **//


// ** STATION_CONFIGTION_RESPONSE **//
pub fn crate_station_configuration_response(seq_num: u8) -> std::io::Result<Vec<u8>> {

    let result_code = Tlv {
        type_: 33,
        length  : ("00000000".len()/2) as u16,
        value: hex::decode("00000000").unwrap(),
    };

    let control_header = CapwapControlHeader {
        message_type_reserved: 0,
        message_type: 26,
        sequence_number: seq_num,
        message_length: 11,
        flags: 0,
    };

    let header = CapwapHeader {
        header_length: 2,
        radio_id: 1,
        wireless_binding_id: 1,
        payload_type: 0,
        fragment: 0,
        last_fragment: 0,
        wireless_header: 0,
        radio_mac_header: 0,
        keep_alive: 0,
        reserved: 0,
        fragment_id: 0,
        fragment_offset: 0,
        reserved2: 0,
    };

    let preamble = CapwapPreamble {
        version: 0,
        type_: 0,
    };

    let mut station_configuration_response_bytes = preamble.to_bytes()?;
    station_configuration_response_bytes.append(&mut header.to_bytes()?);
    station_configuration_response_bytes.append(&mut control_header.to_bytes()?);
    station_configuration_response_bytes.append(&mut result_code.to_bytes()?);
    Ok(station_configuration_response_bytes)
}

// ============================================================================
// 辅助构造函数（供 ap 模块使用）
// ============================================================================

use crate::infra::util;

/// 构造 WtpEventRequest 报文（STA 上线流程用）
pub fn build_wtp_event(sta_mac: &str, sta_ip: &str, event_type: WtpEventType) -> Vec<u8> {
    crate_wtp_event_request(sta_mac, sta_ip, event_type)
        .expect("Failed to build WtpEvent")
}

/// 构造 KeepAlive 报文（Data 通道，接受字节数组 MAC）
pub fn build_keepalive_raw(mac: &[u8; 6]) -> Vec<u8> {
    let s = util::mac_bytes_to_hex(mac);
    crate_keepalive(&s).expect("Failed to build KeepAlive")
}

/// 构造 Echo Request 报文（Control 通道，接受字节数组 MAC）
pub fn build_echo_raw(mac: &[u8; 6]) -> Vec<u8> {
    let s = util::mac_bytes_to_hex(mac);
    crate_echo_request(&s).expect("Failed to build Echo")
}
// ** STATION_CONFIGTION_RESPONSE **//