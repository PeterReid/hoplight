use std::io::Cursor;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::iter;
use crypto::chacha20::ChaCha20;
use crypto::symmetriccipher::{SynchronousStreamCipher, SeekableStreamCipher};
use crypto::ed25519;
use crypto::chacha20poly1305::ChaCha20Poly1305;
use crypto::aead::{AeadDecryptor};
use agent::HandleError;
use identity::Identity;
use expected_packet_set::{ExpectedPacket, ExpectedPacketSet};

pub enum Direction {
    Incoming,
    Outgoing,
}

pub struct Stream{
    pub key: [u8; 32],
    pub neighbor_is_lexico_later: bool,
    
    // It happens to be efficient to generate 8 message identifiers at a time, so we store 
    // the current 8 in a buffer. 
    // This should be private once this structure has been thought through.
    pub outgoing_message_identifiers: [u64; 8],
    pub outgoing_message_index: u64,
    
    pub incoming_message_mask_start: u64,
    pub incoming_message_mask: u64,
}

impl Stream {
    pub fn maybe_new(own_seed: &Option<[u8; 32]>, neighbor_key_material: &Option<[u8; 32]>, stream_with: &Identity, neighbor_is_lexico_later: bool, upcoming_packets: &mut ExpectedPacketSet) -> Option<Stream> {
        if let (Some(ref own_seed), Some(ref neighbor_key_material)) = (*own_seed, *neighbor_key_material) {
            let (stream_private, _stream_public) = ed25519::keypair(&own_seed[..]);
            let stream = Stream {
                key: ed25519::exchange(&neighbor_key_material[..], &stream_private[..]),
                outgoing_message_identifiers: [0u64; 8],
                outgoing_message_index: 0,
                neighbor_is_lexico_later: neighbor_is_lexico_later,
                incoming_message_mask_start: 0,
                incoming_message_mask: 0xffff_ffff_ffff_ffff,
            };
            
            let mut some_identifiers = [0u64; 64];
            stream.generate_identifiers(Direction::Incoming, 0, &mut some_identifiers);
            for (idx, identifier) in some_identifiers.iter().enumerate() {
                if idx==0 {
                    println!("We expect the first packet identifier from {:?} to be {}", stream_with, identifier) 
                }
                upcoming_packets.add(
                    ExpectedPacket{
                        stream_with: *stream_with,
                        stream_key: stream.key,
                        packet_number: idx as u64,
                    },
                    *identifier
                );
            }
            
            Some(stream)
        } else {
            None
        }
    }

    pub fn generate_identifiers(
        &self,
        direction: Direction,
        initial_identifier_number: u64,
        identifiers: &mut [u64]
    ) {
        let nonce = match (direction, self.neighbor_is_lexico_later) {
            (Direction::Incoming, true) | (Direction::Outgoing, false) => [0xff, 0xff, 0xff, 0xff, 0x11, 0x11, 0x11, 0x11],
            (Direction::Incoming, false) | (Direction::Outgoing, true) => [0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00],
        };
        
        let mut identifier_generator = ChaCha20::new(&self.key[..], &nonce[..]);
        identifier_generator.seek(8 * initial_identifier_number).unwrap();
        let buffer_size = 8 * identifiers.len();
        let zeros: Vec<u8> = iter::repeat(0).take(buffer_size).collect();
        let mut buffer: Vec<u8> = iter::repeat(0).take(buffer_size).collect();
        identifier_generator.process(&zeros[..], &mut buffer[..]);
        
        let mut u64_reader = Cursor::new(buffer);
        for identifier in identifiers.iter_mut() {
            *identifier = u64_reader.read_u64::<LittleEndian>().unwrap()
        }
    }
    
    fn make_keystream(&self, nonce: u64) -> ChaCha20Poly1305 {
        let mut buf = [0u8; 8];
        {
            let mut cursor = Cursor::new(&mut buf[..]);
            cursor.write_u64::<LittleEndian>(nonce).unwrap();
        }
        
        ChaCha20Poly1305::new(&self.key[..], &buf[..], &[])
    }
    
    pub fn produce_outgoing_identifier(&mut self) -> (u64, ChaCha20Poly1305) {
        if (self.outgoing_message_index % 8) == 0 {
            // Generate a new batch!
            let mut identifiers_temp = [0; 8];
            self.generate_identifiers(Direction::Outgoing, self.outgoing_message_index, &mut identifiers_temp);
            self.outgoing_message_identifiers = identifiers_temp;
        }
        
        let index = self.outgoing_message_index;
        let identifier = self.outgoing_message_identifiers[(self.outgoing_message_index % 8) as usize];

        self.outgoing_message_index += 1;
        let index_offset = if self.neighbor_is_lexico_later { 1 } else { 0 };
        (identifier, self.make_keystream(index*2 + index_offset))
    }
    
    pub fn decrypt_incoming_payload(&mut self, packet_number: u64, encrypted: &[u8], checksum: &[u8]) -> Result<Vec<u8>, HandleError> {
        let index_offset = if self.neighbor_is_lexico_later { 0 } else { 1 };
        
        let mut keystream = self.make_keystream(packet_number*2 + index_offset);
        let mut output: Vec<u8> = iter::repeat(0).take(encrypted.len()).collect();
        if keystream.decrypt(encrypted, &mut output[..], checksum) {
            Ok(output)
        } else {
            Err(HandleError::BadChecksum)
        }
    }
    
    
    pub fn got_incoming_packet(
        &mut self,
        packet: &ExpectedPacket,
        expected_packet_set: &mut ExpectedPacketSet
    ){
        if packet.packet_number < self.incoming_message_mask_start {
            return;
        }
        
        let bit_offset_in_mask = packet.packet_number - self.incoming_message_mask_start;
        if bit_offset_in_mask > 64 {
            // This is surprising... we did not generate this far ahead.
            panic!("Received incoming packet that we did not mean to generate yet.");
        }
        
        self.incoming_message_mask = self.incoming_message_mask & !(1u64 << bit_offset_in_mask);
       
        if self.incoming_message_mask & 0xff == 0 || (self.incoming_message_mask>>48) != 0xffff {
            let mut incoming_identifiers = [0u64; 8];
            self.generate_identifiers(Direction::Incoming, self.incoming_message_mask_start + 64, &mut incoming_identifiers[..]);
            
            for (idx, incoming_identifier) in incoming_identifiers.iter().enumerate() {
                expected_packet_set.add(ExpectedPacket{
                    stream_with: packet.stream_with,
                    stream_key: self.key,
                    packet_number: self.incoming_message_mask_start + 64 + (idx as u64),
                }, *incoming_identifier);
            }
            
            if self.incoming_message_mask != 0 {
                // There were some packets that we expected to receive but did not. We'd better clear them
                // out from the expected packet set.
                let mut abandoned_incoming_identifiers = [0u64; 8];
                self.generate_identifiers(Direction::Incoming, self.incoming_message_mask_start, &mut abandoned_incoming_identifiers[..]);
                for (idx, abandoned_incoming_identifier) in abandoned_incoming_identifiers.iter().enumerate() {
                    expected_packet_set.remove(&ExpectedPacket{
                        stream_with: packet.stream_with,
                        stream_key: self.key,
                        packet_number: self.incoming_message_mask_start + (idx as u64),
                    }, *abandoned_incoming_identifier);
                }
            }
            
            self.incoming_message_mask = (self.incoming_message_mask >> 8) | (0xff<<56);
            self.incoming_message_mask_start += 8;
        }
    }

}


pub struct StreamCluster {
    neighbor: Identity,
    
    neighbor_is_lexico_later: bool,
    
    /// We track whether or not the neighbor has sent us something encrypted using
    /// our new current seed. Until they do, we don't know whether or not that
    /// packet made it through, so we continue to send using the previous if we can.
    own_current_acknowledged: bool, 
    
    /// A secret, known only by us, which was used to generate our own keypair for
    /// this stream. Keeping the secret around is useful for recomputing the symmetric
    /// key when the neighbor changes their public key.
    ///
    /// TODO: This is only used infrequently, so it doesn't really need to be always
    /// in memory with the symmetric key.
    own_current_seed: Option<[u8; 32]>,
    own_previous_seed: Option<[u8; 32]>,
    
    /// Neighbor's public key generated for this stream. The corresponding private key
    /// is known only by the neighbor. 
    ///
    /// This is *not* the neighbor's permanent identifier (which also happens to be a
    /// public key).
    ///
    /// TODO: This is only used infrequently, so it doesn't really need to be always
    /// in memory with the symmetric key.
    neighbor_current_key_material: Option<[u8; 32]>,
    neighbor_previous_key_material: Option<[u8; 32]>,
    
    own_current_neighbor_current: Option<Stream>,
    own_current_neighbor_previous: Option<Stream>,
    own_previous_neighbor_current: Option<Stream>,
    own_previous_neighbor_previous: Option<Stream>,
}

impl StreamCluster {
    pub fn new(neighbor: &Identity, neighbor_is_lexico_later: bool) -> StreamCluster {
        StreamCluster {
            neighbor: *neighbor,
            neighbor_is_lexico_later: neighbor_is_lexico_later,
            own_current_acknowledged: false,
            
            own_current_seed: None,
            own_previous_seed: None,
            neighbor_current_key_material: None,
            neighbor_previous_key_material: None,
            
            own_current_neighbor_current: None,
            own_current_neighbor_previous: None,
            own_previous_neighbor_current: None,
            own_previous_neighbor_previous: None,
        }
    }
    
    pub fn push_own_seed(&mut self, seed: &[u8; 32], upcoming_packets: &mut ExpectedPacketSet) {
        self.own_previous_seed = self.own_current_seed.take();
        self.own_current_seed = Some(*seed);
        
        self.own_previous_neighbor_current = self.own_current_neighbor_current.take();
        self.own_previous_neighbor_previous = self.own_current_neighbor_previous.take();
        
        self.own_current_neighbor_current = Stream::maybe_new(&self.own_current_seed, &self.neighbor_current_key_material, &self.neighbor, self.neighbor_is_lexico_later, upcoming_packets);
        self.own_current_neighbor_previous = Stream::maybe_new(&self.own_current_seed, &self.neighbor_previous_key_material, &self.neighbor, self.neighbor_is_lexico_later, upcoming_packets);
    }
    
    pub fn push_neighbor_key_material(&mut self, neighbor_key_material: &[u8; 32], upcoming_packets: &mut ExpectedPacketSet) {
        self.neighbor_previous_key_material = self.neighbor_current_key_material.take();
        self.neighbor_current_key_material = Some(*neighbor_key_material);
        
        self.own_current_neighbor_previous = self.own_current_neighbor_current.take();
        self.own_previous_neighbor_previous = self.own_previous_neighbor_current.take();
        
        self.own_current_neighbor_current = Stream::maybe_new(&self.own_current_seed, &self.neighbor_current_key_material, &self.neighbor,  self.neighbor_is_lexico_later, upcoming_packets);
        self.own_previous_neighbor_current = Stream::maybe_new(&self.own_previous_seed, &self.neighbor_current_key_material, &self.neighbor, self.neighbor_is_lexico_later, upcoming_packets);
    }
    
    pub fn produce_outgoing_identifier(&mut self) -> Result<(u64, ChaCha20Poly1305), HandleError> {
        let (preferred, backup) = if self.own_current_acknowledged {
            (self.own_current_neighbor_current.as_mut(), self.own_previous_neighbor_current.as_mut())
        } else {
            (self.own_previous_neighbor_current.as_mut(), self.own_current_neighbor_current.as_mut())
        };
        
        let chosen: Option<&mut Stream> = preferred.or(backup);
        if let Some(chosen) = chosen {
            Ok(chosen.produce_outgoing_identifier())
        } else {
            Err(HandleError::StreamNotReady)
        }
    }
    
    pub fn decrypt_incoming_payload(&mut self, stream_key: &[u8; 32], packet_number: u64, payload: &[u8], checksum: &[u8]) -> Result<Vec<u8>, HandleError> {
        let mut streams = [
            &mut self.own_current_neighbor_current,
            &mut self.own_current_neighbor_previous,
            &mut self.own_previous_neighbor_current,
            &mut self.own_previous_neighbor_previous,
        ];
        for ref mut stream in streams.iter_mut() {
            if let Some(stream) = stream.as_mut() {
                if *stream_key == stream.key {
                    return stream.decrypt_incoming_payload(packet_number, payload, checksum);
                }
            }
        }
        return Err(HandleError::UnrecognizedPacket);
    }
    
    pub fn got_incoming_packet(
        &mut self, 
        packet: &ExpectedPacket, 
        packet_identifier: u64,
        upcoming: &mut ExpectedPacketSet
    ) {
        let mut streams = [
            &mut self.own_current_neighbor_current,
            &mut self.own_current_neighbor_previous,
            &mut self.own_previous_neighbor_current,
            &mut self.own_previous_neighbor_previous,
        ];
        
        for stream in streams.iter_mut() {
            if let Some(stream) = stream.as_mut() {
                if stream.key == packet.stream_key {
                    stream.got_incoming_packet(packet, upcoming)
                }
            }
        }
        
        upcoming.remove(packet, packet_identifier);
    }
}

