use agent::HandleError;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use identity::Identity;
use std::io::{Cursor, Write};

const INNER_PUBLIC_KEY_START: usize = 0;
const INNER_PUBLIC_KEY_LEN: usize = 32;
const INNER_PUBLIC_KEY_END: usize = INNER_PUBLIC_KEY_START + INNER_PUBLIC_KEY_LEN;

const INNER_TIMESTAMP_START: usize = INNER_PUBLIC_KEY_END;
const INNER_TIMESTAMP_LEN: usize = 8;
const INNER_TIMESTAMP_END: usize = INNER_TIMESTAMP_START + INNER_TIMESTAMP_LEN;

const INNER_SIGNATURE_START: usize = INNER_TIMESTAMP_END;
const INNER_SIGNATURE_LEN: usize = 64;
const INNER_SIGNATURE_END: usize = INNER_SIGNATURE_START + INNER_SIGNATURE_LEN;

pub const INNER_LEN: usize = INNER_SIGNATURE_END;


const OUTER_EPHEMERAL_PUBLIC_KEY_START: usize = 0;
const OUTER_EPHEMERAL_PUBLIC_KEY_LEN: usize = 32;
const OUTER_EPHEMERAL_PUBLIC_KEY_END: usize = OUTER_EPHEMERAL_PUBLIC_KEY_START + OUTER_EPHEMERAL_PUBLIC_KEY_LEN;

/// Start of the "inner" section of the overall initiation packet
const OUTER_INNER_START: usize = OUTER_EPHEMERAL_PUBLIC_KEY_END;
const OUTER_INNER_LEN: usize = INNER_LEN;
const OUTER_INNER_END: usize = OUTER_INNER_START + OUTER_INNER_LEN;

const OUTER_AUTHENTICATOR_START: usize = OUTER_INNER_END;
const OUTER_AUTHENTICATOR_LEN: usize = 16;
const OUTER_AUTHENTICATOR_END: usize = OUTER_AUTHENTICATOR_START + OUTER_AUTHENTICATOR_LEN;

const OUTER_LEN: usize = OUTER_AUTHENTICATOR_END;

pub struct InitiationPacketOuter<'a> {
    pub ephemeral_public_key: &'a [u8; OUTER_EPHEMERAL_PUBLIC_KEY_LEN],
    pub inner: &'a [u8; OUTER_INNER_LEN],
    pub authenticator: &'a [u8; OUTER_AUTHENTICATOR_LEN],
}

impl<'a> InitiationPacketOuter<'a> {
    pub fn decode(packet: &[u8]) -> Result<InitiationPacketOuter, HandleError> {
        if packet.len() < OUTER_LEN {
            // Too short! We should not have tried to decode this.
            return Err(HandleError::InternalError);
        }
        
        Ok(InitiationPacketOuter{
            ephemeral_public_key: array_ref!(packet, OUTER_EPHEMERAL_PUBLIC_KEY_START, OUTER_EPHEMERAL_PUBLIC_KEY_LEN),
            inner: array_ref!(packet, OUTER_INNER_START, OUTER_INNER_LEN),
            authenticator: array_ref!(packet, OUTER_AUTHENTICATOR_START, OUTER_AUTHENTICATOR_LEN),
        })
    }
}

pub struct InitiationPacketInner<'a> {
    pub public_key: &'a [u8; INNER_PUBLIC_KEY_LEN],
    pub timestamp: u64,
    pub signature: &'a [u8; INNER_SIGNATURE_LEN],
}

impl<'a> InitiationPacketInner<'a> {
    pub fn decode(packet: &[u8; OUTER_INNER_LEN]) -> InitiationPacketInner {
        InitiationPacketInner{
            public_key: array_ref!(packet, INNER_PUBLIC_KEY_START, INNER_PUBLIC_KEY_LEN),
            timestamp: (&packet[INNER_TIMESTAMP_START..INNER_TIMESTAMP_END]).read_u64::<LittleEndian>().unwrap(),
            signature: array_ref!(packet, INNER_SIGNATURE_START, INNER_SIGNATURE_LEN),
        }
    }
}


pub struct Signable<'a> {
    pub timestamp: u64,
    pub sender: &'a Identity,
    pub recipient: &'a Identity,
    pub key_material: &'a [u8; 32],
    pub symmetric_key: &'a [u8; 32],
}

impl<'a> Signable<'a> {
    pub fn as_bytes(&self) -> [u8; 136] {
        let mut result = [0u8; 8 + 32 + 32 + 32 + 32];
        {
            let mut cursor = Cursor::new(&mut result[..]);
            cursor.write_all(&self.key_material[..]).unwrap();
            cursor.write_all(self.symmetric_key).unwrap();
            cursor.write_all(&self.sender.as_bytes()[..]).unwrap();
            cursor.write_all(&self.recipient.as_bytes()[..]).unwrap();
            cursor.write_u64::<LittleEndian>(self.timestamp).unwrap();
        }
        result
    }
}

pub struct InnerParams<'a> {
    pub timestamp: u64,
    pub sender: &'a Identity,
    pub signature: &'a [u8; 64],
}

impl<'a> InnerParams<'a> {
    pub fn as_bytes(&self) -> [u8; INNER_LEN] {
        let mut result = [0u8; 32 + 8 + 64];
        {
            let mut cursor = Cursor::new(&mut result[..]);
            cursor.write_all(&self.sender.as_bytes()[..]).unwrap();
            cursor.write_u64::<LittleEndian>(self.timestamp).unwrap();
            cursor.write_all(&self.signature[..]).unwrap();
        }
        result
    }
}

