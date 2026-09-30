use byteorder::{BigEndian,WriteBytesExt};
pub struct WtpMacType {
    pub mac_type: u16,
    pub length: u16,
    pub value: u8,
}

impl WtpMacType {
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let mut bytes = vec![];
        bytes.write_u16::<BigEndian>(self.mac_type)?;
        bytes.write_u16::<BigEndian>(self.length)?;
        bytes.write_u8(self.value)?;
        Ok(bytes)
    }
}