use identity::Identity;
use ip_address_port::IpAddressPort;

use crypto::chacha20poly1305::ChaCha20Poly1305;
use crypto::ed25519;
use crypto::aead::{AeadEncryptor, AeadDecryptor};

use std::collections::HashMap;
use std::io::Cursor;

use byteorder::{LittleEndian, WriteBytesExt};
use std::iter;
use vm::{self, Vm};
use content_packet::ContentPacket;
use initiation_packet::{self, InitiationPacketInner, InitiationPacketOuter};
use rand::Rng;

struct NeighborState {
    address: IpAddressPort,
    
    symmetric_key: [u8; 32],
    
    /// Increases by two for each message, staying odd if the neighbor's public key is
    /// lexicographically greater than ours and staying even otherwise.
    incoming_message_nonce: u64,
    
    /// Increases by two for each message, staying even if the neighbor's public key is
    /// lexicographically greater than ours and staying odd otherwise.
    outgoing_message_nonce: u64,
    
    /// Neighbor's public key generated for this stream. The corresponding private key
    /// is known only by the neighbor. 
    ///
    /// This is *not* the neighbor's permanent identifier (which also happens to be a
    /// public key).
    ///
    /// TODO: This is only used infrequently, so it doesn't really need to be always
    /// in memory with the symmetric key.
    neighbor_public_key: [u8; 32],
    
    /// A secret, known only by us, which was used to generate our own keypair for
    /// this stream. Keeping the secret around is useful for recomputing the symmetric
    /// key when the neighbor changes their public key.
    ///
    /// TODO: This is only used infrequently, so it doesn't really need to be always
    /// in memory with the symmetric key.
    own_seed: [u8; 32],
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

pub trait AgentEnvironment {
    fn get_current_timestamp(&self) -> u64;
    fn send(&mut self, address: &IpAddressPort, packet: &[u8]);
}

impl AgentEnvironment {
}

pub struct Agent<E>{
    identity: Identity,
    private_key: [u8; 64],
    neighbors: HashMap<Identity, NeighborState>,
    upcoming_packet_identifiers: HashMap<u64, (Identity, u64)>,
    environment: E,
}

#[derive(Debug)]
pub enum HandleError {
    UnrecognizedPacket,
    UnrecognizedNeighbor,
    InternalLimitExceeded,
    BadChecksum,
    BadSignature,
    InternalError,
    CannotStreamWithSelf,
}

pub const CONTENTFUL_PACKET_THRESHOLD: usize = 
    8 + // packet identifier
    4 + // length
    16 + // checksum
    200 // minimum payload length
;


impl<E:AgentEnvironment+Rng> Agent<E> {
    pub fn new(identity_seed: &[u8; 32], environment: E) -> Agent<E> {
        let (private_key, identity_bytes) = ed25519::keypair(&identity_seed[..]);
        Agent{
            identity: Identity::from_bytes(&identity_bytes),
            private_key: private_key,
            neighbors: HashMap::new(),
            upcoming_packet_identifiers: HashMap::new(),
            environment: environment,
        }
    }

    pub fn handle_packet(&mut self, source: &IpAddressPort, packet: &[u8]) {
        if packet.len() >= CONTENTFUL_PACKET_THRESHOLD {
            match self.handle_contentful_packet(packet) {
                _ => {
                    println!("TODO");
                }
            }
        } else {
            match self.handle_initiation_packet(source, packet) {
                Err(e) => {
                    println!("handle_initiation_packet failed: {:?}", e);
                }
                _ => {
                    println!("TODO");
                }
            }
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
    
    pub fn check_timestamp(&self, _timestamp: u64) -> Result<(), HandleError> {
        // TODO: If this is too different from now, return a BadTimestamp error.
        Ok( () )
    }
    
    pub fn handle_initiation_packet(&mut self, source: &IpAddressPort, packet: &[u8]) -> Result<(), HandleError> {
        let parts = try!(InitiationPacketOuter::decode(packet));
        let symmetric_key: [u8; 32] = ed25519::exchange(parts.ephemeral_public_key, &self.private_key[..]);
        let mut inner_decrypted = [0u8; initiation_packet::INNER_LEN];
        
        if !ChaCha20Poly1305::new(&symmetric_key[..], &[0xff; 8], &[]).decrypt(parts.inner, &mut inner_decrypted[..], parts.authenticator) {
            return Err(HandleError::BadChecksum);
        }
        
        let inner_parts = InitiationPacketInner::decode(&inner_decrypted);
        let sender_identity = Identity::from_bytes(inner_parts.public_key);
        
        let bytes_to_sign = initiation_packet::Signable{
            timestamp: inner_parts.timestamp,
            sender: &sender_identity,
            recipient: &self.identity,
            key_material: parts.ephemeral_public_key,
            symmetric_key: &symmetric_key,
        }.as_bytes();
        
        if !ed25519::verify(&bytes_to_sign[..], inner_parts.public_key, inner_parts.signature) {
            return Err(HandleError::BadSignature);
        }
        
        try!(self.check_timestamp(inner_parts.timestamp));
        
        // There might eventually be a policy decision here where we decide whether or not it is worth keeping this
        // this new neighbor in memory. For now, we will just accept them.
        
        if let Some(neighbor_state) = self.neighbors.get_mut(&sender_identity) {
            let (stream_private, _stream_public) = ed25519::keypair(&neighbor_state.own_seed[..]);
            
            neighbor_state.address = *source;
            neighbor_state.neighbor_public_key = *parts.ephemeral_public_key;
            neighbor_state.symmetric_key = ed25519::exchange(parts.ephemeral_public_key, &stream_private[..]);
            
            return Ok( () )
        }
        
        let neighbor_is_later = try!(sender_identity.is_greater_than(&self.identity).map_err(|_| HandleError::CannotStreamWithSelf));
        let own_seed = { let mut bs = [0u8;32]; self.environment.fill_bytes(&mut bs); bs };
        
        let (stream_private, stream_public) = ed25519::keypair(&own_seed[..]);
        let stream_symmetric_key = ed25519::exchange(parts.ephemeral_public_key, &stream_private[..]); 
        
        self.send_initiation_packet(&sender_identity, source, &own_seed);
        
        self.neighbors.insert(sender_identity, NeighborState {
            address: *source,
            symmetric_key: stream_symmetric_key,
            incoming_message_nonce: if neighbor_is_later { 1 } else { 0 },
            outgoing_message_nonce: if neighbor_is_later { 0 } else { 1 },
            neighbor_public_key: *parts.ephemeral_public_key,
            own_seed: own_seed,
        });
        
        Ok( () )
    }
    
    fn form_initiation_packet(&self, neighbor_identity: &Identity, own_seed: &[u8; 32]) -> [u8; 152] {
        let (stream_private, stream_public) = ed25519::keypair(&own_seed[..]);
        let symmetric_key = ed25519::exchange(&neighbor_identity.as_bytes()[..], &stream_private[..]);
        let now = self.environment.get_current_timestamp();
        
        let inner = initiation_packet::Signable{
            timestamp: now,
            sender: &self.identity,
            recipient: neighbor_identity,
            key_material: &stream_public,
            symmetric_key: &symmetric_key
        };
        
        let inner_decrypted = initiation_packet::InnerParams{
            timestamp: now,
            sender: &self.identity,
            signature: &ed25519::signature(&inner.as_bytes(), &self.private_key)
        }.as_bytes();
        
        let mut result = [0u8; 32 + 104 + 16];
        {
            let (key_material_buffer, remainder) = (&mut result[..]).split_at_mut(32);
            let (inner_encrypted, tag) = remainder.split_at_mut(104);
            
            ChaCha20Poly1305::new(&symmetric_key, &[0xff; 8], &[]).encrypt(&inner_decrypted[..], inner_encrypted, tag);
            for (dest, src) in key_material_buffer.iter_mut().zip(stream_public.iter()) {
                *dest = *src;
            }
        }
        
        result
    }
    
    fn send_initiation_packet(&mut self, neighbor_identity: &Identity, neighbor_location: &IpAddressPort, own_seed: &[u8; 32]) {
        let packet = self.form_initiation_packet(neighbor_identity, own_seed);
        self.environment.send(neighbor_location, &packet[..]);
    }
    
    fn initiate_stream_with(&mut self, neighbor_identity: &Identity, neighbor_location: &IpAddressPort) {
        let own_seed = { let mut bs = [0u8;32]; self.environment.fill_bytes(&mut bs); bs };
        
        // TODO: This way we're storing state is not really right... We don't have a symmetric key yet.
        let n = NeighborState {
            address: *neighbor_location,
            symmetric_key: [0; 32],
            incoming_message_nonce: 0, // TODO: Does not belong here
            outgoing_message_nonce: 0,
            neighbor_public_key: [0; 32],
            own_seed: own_seed,
        };
        
        let packet = self.form_initiation_packet(neighbor_identity, &own_seed);
        self.environment.send(neighbor_location, &packet[..]);
        
        self.neighbors.insert(*neighbor_identity, n);
    }
}

#[cfg(test)]
mod test{
    use rand::chacha::ChaChaRng;
    use rand::{Rng, SeedableRng};
    use super::{Agent, AgentEnvironment};
    use ip_address_port::IpAddressPort;
    use std::collections::HashMap;
    
    struct DummyEnvironment {
        rng: ChaChaRng,
        outgoing: Vec<(IpAddressPort, Vec<u8>)>,
        location: IpAddressPort,
    }
    
    impl DummyEnvironment {
        fn new(seed: u32, location: IpAddressPort) -> DummyEnvironment {
            DummyEnvironment{
                rng: ChaChaRng::from_seed(&[seed]),
                outgoing: Vec::new(),
                location: location,
            }
        }
    }
    
    impl Rng for DummyEnvironment {
        fn next_u32(&mut self) -> u32 {
            self.rng.next_u32()
        }
    }
    
    impl AgentEnvironment for DummyEnvironment{
        fn get_current_timestamp(&self) -> u64 {
            123456
        }
        
        fn send(&mut self, dest: &IpAddressPort, packet: &[u8]) {
            self.outgoing.push((*dest, packet.to_vec()))
        }
    }

    #[derive(Debug)]
    struct InFlightPacket {
        source: IpAddressPort,
        destination: IpAddressPort,
        contents: Vec<u8>,
    }
    fn drain(packets_in_flight: &mut Vec<InFlightPacket>, env: &mut DummyEnvironment) {
        let mut empty = Vec::new();
        ::std::mem::swap(&mut empty, &mut env.outgoing);
        
        for (destination, contents) in empty.into_iter() {
            packets_in_flight.push(InFlightPacket{
                source: env.location,
                destination: destination,
                contents: contents,
            });
        }
    }
    
    #[test]
    fn initiate() {
        let mut a = Agent::new(
            &[0x93, 0xA6, 0x9B, 0xDD, 0xA2, 0xC5, 0xDD, 0x38, 0xBD, 0x90, 0xC6, 0x53, 0x8A, 0x27, 0x62, 0xB0, 
              0x33, 0xBA, 0x0E, 0x31, 0x01, 0xBD, 0xA0, 0xBA, 0xEC, 0x9F, 0x2F, 0x08, 0xD1, 0x63, 0x6A, 0x3B],
            DummyEnvironment::new(1, IpAddressPort{address: [1,1,1,1, 1,1,1,1, 1,1,1,1, 1,1,1,1], port: 5000}));
        
        let mut b = Agent::new(
            &[0x1F, 0xEF, 0xEE, 0x3E, 0x90, 0x63, 0x75, 0xF0, 0xB8, 0x6B, 0x69, 0xE7, 0x83, 0x99, 0xAB, 0xBF, 
              0x35, 0x8B, 0xAD, 0x0A, 0x46, 0x3A, 0x73, 0x60, 0x82, 0xB2, 0x4A, 0x61, 0xF4, 0xEA, 0xA4, 0xBD, ],
            DummyEnvironment::new(2, IpAddressPort{address: [2,2,2,2, 2,2,2,2, 2,2,2,2, 2,2,2,2], port: 5222}));
        
        a.initiate_stream_with(&b.identity, &b.environment.location);
        
        for _ in 0..2 {
            let mut in_flights = Vec::new();
            drain(&mut in_flights, &mut a.environment);
            drain(&mut in_flights, &mut b.environment);
            
            for in_flight in in_flights.into_iter() {
                let dest = 
                    if a.environment.location == in_flight.destination {
                        &mut a
                    } else if b.environment.location == in_flight.destination {
                        &mut b
                    } else {
                        continue;
                    };
                
                dest.handle_packet(&in_flight.source, &in_flight.contents[..]);
                
            }
        }
        
        panic!("see");
    }

}

