use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::Cursor;
use std::cmp::{Ord, PartialOrd, Ordering};

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct Identity{
    words: [u32; 8]
}

impl Identity{
    pub fn from_bytes(bs: &[u8; 32]) -> Identity {
        let mut words = [0; 8];
        let mut cursor = Cursor::new(&bs[..]);
        
        for word in words.iter_mut() {
            *word = cursor.read_u32::<LittleEndian>().unwrap()
        }
        
        Identity{
            words: words
        }
    }
    
    pub fn as_bytes(&self) -> [u8; 32] {
        let mut bs = [0u8; 32];
        {
            let mut cursor = Cursor::new(&mut bs[..]);
            
            for word in self.words.iter() {
                cursor.write_u32::<LittleEndian>(*word).unwrap();
            }
        }
        
        bs
    }
    
    /// Compare with another Identity, where it is an error to match.
    pub fn is_greater_than(&self, other: &Identity) -> Result<bool, ()> {
        match self.cmp(other) {
            Ordering::Equal => Err( () ),
            Ordering::Less => Ok(false),
            Ordering::Greater => Ok(true),
        }
    }
}

impl PartialOrd for Identity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.words.partial_cmp(&other.words)
    }
}

impl Ord for Identity {
    fn cmp(&self, other: &Self) -> Ordering {
        self.words.cmp(&other.words)
    }
}