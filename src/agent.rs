use identity::Identity;
use ip_address_port::IpAddressPort;

use crypto::chacha20poly1305::ChaCha20Poly1305;
use crypto::aead::{/*AeadEncryptor,*/AeadDecryptor};

use std::collections::HashMap;
use std::io::Cursor;

use byteorder::{LittleEndian, WriteBytesExt};
use std::iter;
use vm::{self, Vm};
use content_packet::ContentPacket;

struct NeighborState {
    address: IpAddressPort,
    symmetric_key: [u8; 32],
}

impl NeighborState {
    fn decrypt_payload(&mut self, packet_number: u64, encrypted: &[u8], checksum: &[u8]) -> Result<Vec<u8>, HandleError> {
        // TODO: Handle updating the expected packet identifiers here. Or maybe outside...
        let mut c = ChaCha20Poly1305::new(&self.symmetric_key, &self.make_nonce(packet_number), &[]);
        let mut output: Vec<u8> = iter::repeat(0).take(encrypted.len()).collect();
        if c.decrypt(encrypted, &mut output[..], checksum) {
            Ok(output)
        } else {
            Err(HandleError::BadChecksum)
        }
    }
    
    fn make_nonce(&self, packet_number: u64) -> [u8; 8] {
        let mut buf = [0u8; 8];
        {
            let mut cursor = Cursor::new(&mut buf[..]);
            cursor.write_u64::<LittleEndian>(packet_number).unwrap();
        }
        buf
    }
}

pub struct Agent{
    identity: Identity,
    neighbors: HashMap<Identity, NeighborState>,
    upcoming_packet_identifiers: HashMap<u64, (Identity, u64)>,
}

pub enum HandleError {
    UnrecognizedPacket,
    UnrecognizedNeighbor,
    InternalLimitExceeded,
    BadChecksum,
    InternalError,
}

pub const CONTENTFUL_PACKET_THRESHOLD: usize = 
    8 + // packet identifier
    4 + // length
    16 + // checksum
    4 // minimum payload length
;

impl Agent{
    pub fn handle_packet(&mut self, packet: &[u8]) {
        if packet.len() >= CONTENTFUL_PACKET_THRESHOLD {
            match self.handle_contentful_packet(packet) {
                _ => {
                    println!("TODO");
                }
            }
        } else {
            self.handle_initiation_packet(packet)
        }
    }
    
    fn look_up_packet_identifier(&mut self, packet_identifier: u64) -> Result<(Identity, u64), HandleError> {
        // TODO: I am not sure I actually want to remove this packet identifier just yet. That makes
        // it easy for an intermediary that can watch for packet identifiers to mess up a stream.
        if let Some((stream_with, packet_identifier)) = self.upcoming_packet_identifiers.remove(&packet_identifier) {
            Ok((stream_with, packet_identifier))
        } else {
            Err(HandleError::UnrecognizedPacket)
        }
    }
    
    fn look_up_neighbor_state<'a>(&'a mut self, neighbor: &Identity) -> Result<&'a mut NeighborState, HandleError> {
        if let Some(neighbor_state) = self.neighbors.get_mut(neighbor) {
            Ok(neighbor_state)
        } else {
            Err(HandleError::UnrecognizedNeighbor)
        }
    }
    
    pub fn handle_contentful_packet(&mut self, packet: &[u8]) -> Result<(), HandleError> {
        let parts = try!(ContentPacket::decode(packet));
        
        let (stream_with, packet_number) = try!(self.look_up_packet_identifier(parts.packet_identifier));
        
        let mut neighbor_state = try!(self.look_up_neighbor_state(&stream_with));
        
        let payload = try!(neighbor_state.decrypt_payload(packet_number, parts.encrypted_payload, parts.checksum));
        let payload_words = vm::le_bytes_to_words(&payload);
        
        Vm::new(&payload_words);//.exec();
        
        Ok( () )
    }
    
    pub fn handle_initiation_packet(&mut self, packet: &[u8]) {
    
    }
}
