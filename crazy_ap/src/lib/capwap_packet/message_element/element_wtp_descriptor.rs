use byteorder::{BigEndian,WriteBytesExt};
use crate::capwap_packet::tlv::{Tlv, VendorTlv};

pub struct WtpDescriptor {
    pub type_: u16,
    pub length: u16,
    pub max_radius: u8,
    pub radio_in_use: u8,
    pub encryption_capabilities_number: u8,
    pub encryption_capabilities_encrypt: u8,
    pub encryption_capabilities_encrypt_wbid:u8,
    pub encryption_capabilities: u16,
    pub wtp_descriptor_vendor: u32,
    pub wtp_hardware_version: WtpDescriptorValue,
    pub wtp_active_software_version: WtpDescriptorValue,
    pub wtp_boot_version: WtpDescriptorValue,
    pub wtp_other_softer_version: WtpDescriptorValue,
    pub unknown_wtp_descriptor: WtpDescriptorValue,
}

pub enum WtpDescriptorValue {
    WtpHardwareVersion(Tlv),
    WtpActiveSoftwareVersion(VendorTlv),
    WtpBootVersion(VendorTlv),
    WtpOtherSofterVersion(VendorTlv),
    UnknownWtpDescriptor(VendorTlv),
}


impl WtpDescriptorValue {
    fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        match self {
            WtpDescriptorValue::WtpHardwareVersion(v) => v.to_bytes(),
            WtpDescriptorValue::WtpActiveSoftwareVersion(v) => v.to_bytes(),
            WtpDescriptorValue::WtpBootVersion(v) => v.to_bytes(),
            WtpDescriptorValue::WtpOtherSofterVersion(v) => v.to_bytes(),
            WtpDescriptorValue::UnknownWtpDescriptor(v) => v.to_bytes(),
        }
    } 
}

impl WtpDescriptor {
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let mut bytes = vec![];
        bytes.write_u16::<BigEndian>(self.type_)?;
        bytes.write_u16::<BigEndian>(self.length)?;
        bytes.write_u8(self.max_radius)?;
        bytes.write_u8(self.radio_in_use)?;
        bytes.write_u8(self.encryption_capabilities_number)?;
        let encrypt = self.encryption_capabilities_encrypt << 5 | self.encryption_capabilities_encrypt_wbid;
        bytes.write_u8(encrypt)?;
        bytes.write_u16::<BigEndian>(self.encryption_capabilities)?;
        bytes.write_u32::<BigEndian>(self.wtp_descriptor_vendor)?;
        bytes.append(&mut self.wtp_hardware_version.to_bytes()?);
        bytes.append(&mut self.wtp_active_software_version.to_bytes()?);
        bytes.append(&mut self.wtp_boot_version.to_bytes()?);
        bytes.append(&mut self.wtp_other_softer_version.to_bytes()?);
        bytes.append(&mut self.unknown_wtp_descriptor.to_bytes()?);
        Ok(bytes)
    }
}

