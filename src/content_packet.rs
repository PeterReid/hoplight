use agent::HandleError;
use checked_int_cast::CheckedIntCast;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use rand::Rng;
use std::u32;

pub struct ContentPacket<'a> {
    pub packet_identifier: u64,
    pub encrypted_payload: &'a [u8],
    pub checksum: &'a [u8;16]
}

pub struct ContentPacketWriter<'a> {
    pub encrypted_payload: &'a mut [u8],
    pub checksum: &'a mut [u8;16],
}

const PACKET_IDENTIFIER_START: usize = 0;
const PACKET_IDENTIFIER_LEN: usize = 8;
const PACKET_IDENTIFIER_END: usize = PACKET_IDENTIFIER_START + PACKET_IDENTIFIER_LEN;
const LENGTH_PLUS_START: usize = PACKET_IDENTIFIER_START + PACKET_IDENTIFIER_LEN;
const LENGTH_PLUS_LEN: usize = 4;
const LENGTH_PLUS_END: usize = LENGTH_PLUS_START + LENGTH_PLUS_LEN;
const CHECKSUM_START: usize = LENGTH_PLUS_START + LENGTH_PLUS_LEN;
const CHECKSUM_LEN: usize = 16;
const PAYLOAD_START: usize = CHECKSUM_START + CHECKSUM_LEN;

pub const CONTENTFUL_PACKET_THRESHOLD: usize = 
    8 + // packet identifier
    4 + // length
    16 + // checksum
    0 // minimum payload length. TODO: This will be longer to accomodate 
;

impl<'a> ContentPacket<'a> {
    pub fn decode(packet: &'a [u8]) -> Result<ContentPacket<'a>, HandleError> {
        if packet.len() < CONTENTFUL_PACKET_THRESHOLD {
            return Err(HandleError::InternalError);
        }
        let packet_len = match packet.len().as_u32_checked() {
            Some(packet_len) => packet_len,
            None => { return Err(HandleError::InternalLimitExceeded); }
        };
    
        let packet_identifier = (&packet[PACKET_IDENTIFIER_START..PACKET_IDENTIFIER_END]).read_u64::<LittleEndian>().unwrap();
        let length_words_plus = (&packet[LENGTH_PLUS_START..LENGTH_PLUS_END]).read_u32::<LittleEndian>().unwrap();
        let remaining_bytes = packet_len - PAYLOAD_START as u32;
        let remaining_words = remaining_bytes / 4;
        let checksum = array_ref![packet,CHECKSUM_START,CHECKSUM_LEN];
        let payload_words = length_words_plus % (1 + remaining_words);
        let payload_bytes = (payload_words * 4).as_usize_checked().unwrap();
        let encrypted_payload = &packet[PAYLOAD_START..PAYLOAD_START+payload_bytes];
        
        Ok(ContentPacket{
            packet_identifier: packet_identifier,
            encrypted_payload: encrypted_payload,
            checksum: checksum,
        })
    }
    
    #[allow(dead_code)]
    pub fn prepare<R: Rng>(buffer: &'a mut [u8], payload_length: usize, packet_identifier: u64, rng: &mut R) -> Result<ContentPacketWriter<'a>, HandleError> {
        
        // Defensively zero the buffer. Although every byte of it *should* be overwritten later,
        // we should not risk a defect elsewhere causing something in that buffer to be 
        // overlooked and sent.
        for b in buffer.iter_mut() {
            *b = 0;
        }
        
        let required_length = try!(PAYLOAD_START.checked_add(payload_length).ok_or(HandleError::InternalLimitExceeded));
        if buffer.len() < required_length {
            println!("too short");
            return Err(HandleError::InternalError);
        }
        if payload_length % 4 != 0 {
            return Err(HandleError::InternalError);
        }
        
        let payload_words = try!( (payload_length/4).as_u32_checked().ok_or(HandleError::InternalLimitExceeded) );
        let remaining_bytes = try!((buffer.len() - PAYLOAD_START as usize).as_u32_checked().ok_or(HandleError::InternalLimitExceeded));
        let remaining_words = remaining_bytes / 4;
        // The decoder will have
        // X = payload_words + N*remaining_words - 1
        // to avoid overflowing the u32, 0 <= N < (u32::max - payload_words)/(remaining_words+1)
        let length_extra_n = rng.gen_range(0, (u32::MAX - payload_words) / (remaining_words + 1));
        let encoded_length = payload_words + length_extra_n * (remaining_words + 1);
        
        (&mut buffer[PACKET_IDENTIFIER_START..PACKET_IDENTIFIER_END]).write_u64::<LittleEndian>(packet_identifier).unwrap();
        (&mut buffer[LENGTH_PLUS_START..LENGTH_PLUS_END]).write_u32::<LittleEndian>(encoded_length).unwrap();
        
        let payload_end = PAYLOAD_START + payload_length;
        
        // Fill the padding bytes (at end up buffer) with randomness
        rng.fill_bytes(&mut buffer[payload_end..]);
        
        let buffer_left = &mut buffer[CHECKSUM_START..payload_end];
        
        let (checksum_buffer, payload_buffer) = buffer_left.split_at_mut(CHECKSUM_LEN);
        
        Ok(ContentPacketWriter{
            encrypted_payload: payload_buffer,
            checksum: array_mut_ref![checksum_buffer, 0, CHECKSUM_LEN],
        })
    }
}

#[cfg(test)]
mod test {
    use rand::{XorShiftRng, SeedableRng};
    use super::ContentPacket;
    
    #[test]
    fn read_back() {
        fn read_back_with_lens(buffer_len: usize, payload_len: usize) {
            let mut rng = XorShiftRng::from_seed([
                0xA9797C24, 0x854A3250, 0xF467AD22, 0x2CCE2392
            ]);
            
            let mut xs: Vec<u8> = (0..buffer_len).map(|_| 0).collect();
            let payload: Vec<u8> = (0..payload_len).map(|idx| (idx*3) as u8).collect();
            let checksum: [u8; 16] = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
            let packet_identifier: u64 = 0x0102030405060708;
            
            {
                let writer = ContentPacket::prepare(&mut xs[..], payload.len(), packet_identifier, &mut rng).ok().unwrap();
                
                assert_eq!(writer.encrypted_payload.len(), payload.len());
                for (dest, src) in writer.encrypted_payload.iter_mut().zip(payload.iter()) {
                    *dest = *src;
                }
                for (dest, src) in writer.checksum.iter_mut().zip(checksum.iter()) {
                    *dest = *src;
                }
            }
            
            let read_back = ContentPacket::decode(&xs[..]).ok().unwrap();
            assert_eq!(read_back.packet_identifier, packet_identifier);
            assert_eq!(read_back.encrypted_payload.len(), payload.len());
            assert_eq!(read_back.encrypted_payload.to_vec(), payload);
            assert_eq!(read_back.checksum.to_vec(), checksum.to_vec());
        }
        
        read_back_with_lens(1000, 304);
        read_back_with_lens(1028, 1000);
        read_back_with_lens(28, 0);
        read_back_with_lens(100, 0);
    }

}
