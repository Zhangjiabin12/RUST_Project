use byteorder::{BigEndian,WriteBytesExt};
use crate::capwap_packet::tlv::Tlv;

pub struct WtpBoardData {
    pub type_: u16,
    pub length: u16,
    pub wtp_board_data_vendor: u32,
    pub data_modle_number: WtpBoardDataValue,
    pub data_serial_number: WtpBoardDataValue,
    pub data_board_revision: WtpBoardDataValue,
    pub data_base_mac_address: WtpBoardDataValue,
}


pub enum WtpBoardDataValue {
    WtpModelNumber(Tlv),
    WtpSerialNumber(Tlv),
    BoardRevision(Tlv),
    BaseMacAddress(Tlv),
}


impl WtpBoardDataValue {
    fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        match self {
            WtpBoardDataValue::WtpModelNumber(v) => v.to_bytes(),
            WtpBoardDataValue::WtpSerialNumber(v) => v.to_bytes(),
            WtpBoardDataValue::BoardRevision(v) => v.to_bytes(),
            WtpBoardDataValue::BaseMacAddress(v) => v.to_bytes(),
        }
    }
    
}



impl WtpBoardData {
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let mut bytes = vec![];
        bytes.write_u16::<BigEndian>(self.type_)?;
        bytes.write_u16::<BigEndian>(self.length)?;
        bytes.write_u32::<BigEndian>(self.wtp_board_data_vendor)?;
        bytes.append(&mut self.data_modle_number.to_bytes()?);
        bytes.append(&mut self.data_serial_number.to_bytes()?);
        bytes.append(&mut self.data_board_revision.to_bytes()?);
        bytes.append(&mut self.data_base_mac_address.to_bytes()?);
        Ok(bytes)
    }
}
