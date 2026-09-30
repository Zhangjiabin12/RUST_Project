use byteorder::{WriteBytesExt,BigEndian};
pub struct Ieee80211WtpInfo {
    pub type_: u16,
    pub length: u16,
    pub value: Vec<u8>,
}

impl Ieee80211WtpInfo {
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let mut bytes = vec![];
        bytes.write_u16::<BigEndian>(self.type_)?;
        bytes.write_u16::<BigEndian>(self.length)?;
        bytes.append(&mut self.value.to_vec());
        Ok(bytes)
    }
}