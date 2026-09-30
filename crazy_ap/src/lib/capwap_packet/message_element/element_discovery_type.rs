use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt};
use std::io::Cursor;
// enum MessageElement  {
//     DiscoveryType(DiscoveryType),
// }

// impl MessageElement {
//     fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
//         match self {
//             MessageElement::DiscoveryType(dt) => dt.to_bytes(),
//         }
//     }
// }


pub struct DiscoveryType {
    pub type_: u32,
    pub length: u32,
    pub data: u8,
}

impl DiscoveryType {
    // 将 DiscoveryType 转换为字节序列
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let mut bytes = vec![];
        let message_type_and_length = (self.type_ << 16) | self.length;
        bytes.write_u32::<BigEndian>(message_type_and_length)?;
        bytes.write_u8(self.data)?;
        Ok(bytes)
    }
    pub fn get_data_length(&self) -> u32 {
        5
    }
    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<Self> {
        let mut rdr = Cursor::new(bytes);
        let message_type_and_length = rdr.read_u32::<BigEndian>()?;
        let message_type = message_type_and_length >> 16 & 0x0FFF;
        let length = message_type_and_length & 0x0FFF;
        let data = rdr.read_u8()?;
        Ok(Self {
            type_: message_type,
            length,
            data,
        })
    }
}