use std::io::Cursor;
use byteorder::{LittleEndian, ReadBytesExt};
use std::iter;
use crypto::chacha20::ChaCha20;
use crypto::symmetriccipher::{SynchronousStreamCipher, SeekableStreamCipher};

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
}

impl Stream {
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
    
    pub fn produce_outgoing_identifier(&mut self) -> (u64, u64) {
        if (self.outgoing_message_index % 8) == 0 {
            // Generate a new batch!
            let mut identifiers_temp = [0; 8];
            self.generate_identifiers(Direction::Outgoing, self.outgoing_message_index, &mut identifiers_temp);
            self.outgoing_message_identifiers = identifiers_temp;
        }
        let ret = (self.outgoing_message_index, self.outgoing_message_identifiers[(self.outgoing_message_index % 8) as usize]);
        self.outgoing_message_index += 1;
        ret
    }
}