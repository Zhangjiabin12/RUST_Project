
use byteorder::{BigEndian,WriteBytesExt};
pub struct WtpFrameTunnelMode {
    pub type_: u16,
    pub length: u16,
    pub value: u8,
}
impl WtpFrameTunnelMode {
    // 将 WtpFrameTunnelMode 转换为字节序列
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let mut bytes = vec![];
        bytes.write_u16::<BigEndian>(self.type_)?;
        bytes.write_u16::<BigEndian>(self.length)?;
        bytes.write_u8(self.value)?;
        Ok(bytes)
    }
    
}