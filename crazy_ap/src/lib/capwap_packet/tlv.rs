use byteorder::{WriteBytesExt,BigEndian, ReadBytesExt};
use std::io::{Cursor, Read};

pub struct Tlv {
    pub type_: u16,
    pub length: u16,
    pub value: Vec<u8>,
}

impl Tlv {
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let mut bytes = vec![];
        bytes.write_u16::<BigEndian>(self.type_)?;
        bytes.write_u16::<BigEndian>(self.length)?;
        bytes.append(&mut self.value.to_vec());
        Ok(bytes)
    }
    pub fn from_bytes(bytes : &[u8]) -> std::io::Result<Self> {
        let mut rdr = Cursor::new(bytes);

        let type_ = rdr.read_u16::<BigEndian>()?;
        let length = rdr.read_u16::<BigEndian>()?;
        let mut value = vec![0; length as usize];
        rdr.read_exact(&mut value)?;
        Ok(Self {
            type_,
            length,
            value
        })
    }

    pub fn get_data_length(&self) -> u16 {
        self.length + 4
    }
}

pub struct VendorTlv {
    pub wtp_description: u32,
    pub type_: u16,
    pub length: u16,
    pub value: Vec<u8>,
}

impl VendorTlv {
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let mut bytes = vec![];
        bytes.write_u32::<BigEndian>(self.wtp_description)?;
        bytes.write_u16::<BigEndian>(self.type_)?;
        bytes.write_u16::<BigEndian>(self.length)?;
        bytes.append(&mut self.value.to_vec());
        Ok(bytes)
    }
    
}

pub struct VendorSpciTlv {
    pub type_: u16,
    pub length: u16,
    pub vendor_id: u32,
    pub vendor_element_id: u16,
    pub value: Vec<u8>,

}
impl VendorSpciTlv {
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let mut bytes = vec![];
        bytes.write_u16::<BigEndian>(self.type_)?;
        bytes.write_u16::<BigEndian>(self.length)?;
        bytes.write_u32::<BigEndian>(self.vendor_id)?;
        bytes.write_u16::<BigEndian>(self.vendor_element_id)?;
        bytes.append(&mut self.value.to_vec());
        Ok(bytes)
    }
}