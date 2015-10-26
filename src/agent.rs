use identity::Identity;
use ip_address_port::IpAddressPort;

use crypto::chacha20poly1305::ChaCha20Poly1305;
use crypto::ed25519;
use crypto::aead::{AeadEncryptor, AeadDecryptor};

use std::collections::HashMap;

use std::iter;
use vm::{self, Vm, Fault};
use content_packet::ContentPacket;
use initiation_packet::{self, InitiationPacketInner, InitiationPacketOuter};
use rand::Rng;
use stream::StreamCluster;
use expected_packet_set::{ExpectedPacket, ExpectedPacketSet};

struct NeighborState {
    address: IpAddressPort,
    
    streams: StreamCluster,
}

pub struct Task {
    pub requestor: Identity,
    pub vm: Vm,
}

pub trait AgentEnvironment {
    fn get_current_timestamp(&self) -> u64;
    fn send(&mut self, address: &IpAddressPort, packet: &[u8]);
    
    /// Schedule a task for execution. This will _not_ wait until the task is complete
    /// to return.
    fn execute(&mut self, task: Task);
}

pub struct Agent<E>{
    identity: Identity,
    private_key: [u8; 64],
    neighbors: HashMap<Identity, NeighborState>,
    pub environment: E,
    
    /// Associates expected incoming packet identifiers with the streams they 
    /// may have come from.
    /// Streams are identified by the Identity of their endpoint, their
    /// symmetric key, and their packet index.
    upcoming_packets: ExpectedPacketSet,
}

#[derive(Debug)]
pub enum HandleError {
    UnrecognizedPacket,
    UnrecognizedNeighbor,
    StreamNotReady,
    InternalLimitExceeded,
    BadChecksum,
    BadSignature,
    InternalError,
    CannotStreamWithSelf,
    NotANeighbor,
    VmCreationFailed(Fault),
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
            upcoming_packets: ExpectedPacketSet::new(),
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
    
    pub fn handle_contentful_packet(&mut self, packet: &[u8]) -> Result<(), HandleError> {
        let parts = try!(ContentPacket::decode(packet));
        
        let mut found: Option<(ExpectedPacket, Vec<u8>)> = None;
        
        for expected_packet in self.upcoming_packets.iter(parts.packet_identifier) {
            let neighbor_state = if let Some(neighbor_state) = self.neighbors.get_mut(&expected_packet.stream_with) {
                neighbor_state
            } else {
                // We should not have still had this neighbor as a reason for receiving a packet if it is
                // not in our neighbor list.
                return Err(HandleError::InternalError)
            };
            
            if let Ok(payload) = neighbor_state.streams.decrypt_incoming_payload(&expected_packet.stream_key, expected_packet.packet_number, parts.encrypted_payload, parts.checksum) {
                found = Some( (*expected_packet, payload) );
                break;
            }
        }
        
        let (expected_packet, payload) = 
            if let Some(found) = found { found } 
            else { return Err(HandleError::UnrecognizedPacket) };
        
        self.upcoming_packets.remove(&expected_packet, parts.packet_identifier);
        
        // TODO: Maybe put some new things into upcoming_packets for farther-in-the-future packets.
        
        let payload_words = vm::le_bytes_to_words(&payload);
        
        let vm = try!(Vm::new(&payload_words).map_err(|e| 
            HandleError::VmCreationFailed(e)
        ));
        
        self.environment.execute(Task{ requestor: expected_packet.stream_with, vm: vm});
        
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
        
        let neighbor_is_known = 
            if let Some(neighbor_state) = self.neighbors.get_mut(&sender_identity) {
                neighbor_state.address = *source;
                
                neighbor_state.streams.push_neighbor_key_material(&parts.ephemeral_public_key, &mut self.upcoming_packets);
                
                true
            } else {
                false
            };
        
        if !neighbor_is_known {
            let neighbor_is_later = try!(sender_identity.is_greater_than(&self.identity).map_err(|_| HandleError::CannotStreamWithSelf));
            let own_seed = { let mut bs = [0u8;32]; self.environment.fill_bytes(&mut bs); bs };
            
            self.send_initiation_packet(&sender_identity, source, &own_seed);
            
            let mut n = NeighborState {
                address: *source,
                streams: StreamCluster::new(&sender_identity, neighbor_is_later),
            };
            n.streams.push_neighbor_key_material(&parts.ephemeral_public_key, &mut self.upcoming_packets);
            n.streams.push_own_seed(&own_seed, &mut self.upcoming_packets);
            
            self.neighbors.insert(sender_identity, n);
        }
        
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
    
    pub fn initiate_stream_with(&mut self, neighbor_identity: &Identity, neighbor_location: &IpAddressPort) -> Result<(), HandleError> {
        let own_seed = { let mut bs = [0u8;32]; self.environment.fill_bytes(&mut bs); bs };
     
        let neighbor_is_later = try!(neighbor_identity.is_greater_than(&self.identity).map_err(|_| HandleError::CannotStreamWithSelf));
        
        let mut n = NeighborState {
            address: *neighbor_location,
            streams: StreamCluster::new(neighbor_identity, neighbor_is_later),
        };
        n.streams.push_own_seed( &own_seed, &mut self.upcoming_packets );
        
        let packet = self.form_initiation_packet(neighbor_identity, &own_seed);
        self.environment.send(neighbor_location, &packet[..]);
        
        self.neighbors.insert(*neighbor_identity, n);
        
        Ok( () )
    }
    
    pub fn send_to(&mut self, neighbor: &Identity, payload: &[u8]) -> Result<(), HandleError> {
        let mut neighbor_state = if let Some(x) = self.neighbors.get_mut(neighbor) { x } else {
            return Err(HandleError::NotANeighbor);
        };
        
        let (identifier, mut keystream) = try!(neighbor_state.streams.produce_outgoing_identifier());
        
        let packet_size = payload.len() + CONTENTFUL_PACKET_THRESHOLD; // TODO
        let mut buffer: Vec<u8> = iter::repeat(0).take(packet_size).collect();
        {
            let mut packet_writer = try!(ContentPacket::prepare(&mut buffer[..], payload.len(), identifier, &mut self.environment));
            keystream.encrypt(payload, packet_writer.encrypted_payload, packet_writer.checksum);
        }
        self.environment.send(&neighbor_state.address, &buffer[..]);
        
        Ok( () )
    }
}



#[cfg(test)]
mod test{
    use rand::chacha::ChaChaRng;
    use rand::{Rng, SeedableRng};
    use super::{Agent, AgentEnvironment, Task};
    use ip_address_port::IpAddressPort;
    use vm::{self};

    struct DummyEnvironment {
        rng: ChaChaRng,
        outgoing: Vec<(IpAddressPort, Vec<u8>)>,
        location: IpAddressPort,
        tasks: Vec<Task>,
    }
    
    impl DummyEnvironment {
        fn new(seed: u32, location: IpAddressPort) -> DummyEnvironment {
            DummyEnvironment{
                rng: ChaChaRng::from_seed(&[seed]),
                outgoing: Vec::new(),
                location: location,
                tasks: Vec::new(),
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
        
        fn execute(&mut self, task: Task) {
            self.tasks.push(task);
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
    
    fn exchange(agents: &mut [&mut Agent<DummyEnvironment>]) {
        let mut in_flights = Vec::new();
        for agent in agents.iter_mut() {
            drain(&mut in_flights, &mut agent.environment);
        }
        
        for in_flight in in_flights.into_iter() {
            for maybe_dest in agents.iter_mut() {
                if maybe_dest.environment.location == in_flight.destination {
                    maybe_dest.handle_packet(&in_flight.source, &in_flight.contents[..]);
                }
            }
        }
    }
    
    fn drain_tasks(agents: &mut [&mut Agent<DummyEnvironment>]) {
        for agent in agents.iter_mut() {
            agent.environment.tasks.clear();
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
        
        a.initiate_stream_with(&b.identity, &b.environment.location).ok().expect("initiate_stream_with a->b failed");
        
        for _ in 0..2 {
            exchange(&mut [&mut a, &mut b]);
        }
        
        for round in 0..6 {
            let sample_send = [
                0x11223344, 0x22334455 + round, 0x33445566
            ];
            a.send_to(&b.identity, &vm::words_to_le_bytes(&sample_send)[..]).ok().expect("send_to failed");//&[1,2,3,4]);
            
            assert!(b.environment.tasks.len()==0);
            
            exchange(&mut [&mut a, &mut b]);
            
            assert!(b.environment.tasks.len()==1);
            assert_eq!(b.environment.tasks[0].requestor, a.identity);
            assert_eq!(b.environment.tasks[0].vm.read_memory(1), 0x22334455 + round);
            assert_eq!(b.environment.tasks[0].vm.read_memory(2), 0x33445566);
            
            drain_tasks(&mut [&mut a, &mut b]);
        }
    }

}

