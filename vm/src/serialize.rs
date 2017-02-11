use noun::{Noun, NounKind};

#[derive(Eq, PartialEq, Debug)]
pub enum SerializationError {
    OverlongAtom,
    MaximumLengthExceeded,
}

pub type SerializationResult<T> = Result<T, SerializationError>;

pub struct Serializer {
    atom_encoding: Vec<u8>,
    structure: BitVec,
    maximum_atom_encoding_length: usize,
}

impl Serializer {
    fn new(maximum_atom_encoding_length: usize) -> Serializer {
        Serializer{
            atom_encoding: Vec::new(),
            structure: BitVec::new(),
            maximum_atom_encoding_length: maximum_atom_encoding_length,
        }
    }
    
    fn serialize_atom(&mut self, atom_bytes: &[u8]) -> SerializationResult<()> {
        let len = atom_bytes.len();
        
        if atom_bytes.len()==1 && atom_bytes[0] < 190 {
            if self.atom_encoding.len() >= self.maximum_atom_encoding_length {
                return Err(SerializationError::MaximumLengthExceeded);
            }
            self.atom_encoding.push(atom_bytes[0]);
            return Ok( () );
        }
        
        if len <= 64 {
            self.atom_encoding.push((len as u8) + 190);
        } else {
            self.atom_encoding.push(0xff);
            
            let mut remaining_len = len;
            while remaining_len >= 128 {
                self.atom_encoding.push((remaining_len & 0x7f) as u8 | 0x80);
                remaining_len = remaining_len >> 7;
            }
            self.atom_encoding.push(remaining_len as u8);
        }
        
        if atom_bytes.len() >= self.maximum_atom_encoding_length || self.atom_encoding.len() >= self.maximum_atom_encoding_length - atom_bytes.len() {
            return Err(SerializationError::MaximumLengthExceeded);
        }
        
        for atom_byte in atom_bytes.iter() {
            self.atom_encoding.push(*atom_byte);
        }
        
        Ok( () )
    }

    fn serialize_noun(&mut self, noun: &Noun) -> SerializationResult<()> {
        match noun.as_kind() {
            NounKind::Cell(lhs, rhs) => {
                self.structure.push(true);
                try!(self.serialize_noun(lhs));
                try!(self.serialize_noun(rhs));
            },
            NounKind::Atom(bytes) => {
                self.structure.push(false);
                try!(self.serialize_atom(bytes));
            }
        }
        Ok( () )
    }
}

/// `maximum_atom_encoding_length` sets a rough upper bound on how much memory will be used. It controls
/// how much space all the atoms in the encoding, combined, may take up.
pub fn serialize(noun: &Noun, maximum_atom_encoding_length: usize) -> SerializationResult<Vec<u8>> {
    let mut serializer = Serializer::new(maximum_atom_encoding_length);
    
    try!(serializer.serialize_noun(noun));
    
    let length_encoding = {
        let mut length_encoder = Serializer::new(10);
        try!(length_encoder.serialize_noun(&Noun::from_usize_compact(serializer.atom_encoding.len())));
        length_encoder.atom_encoding
    };
    
    Ok(length_encoding.into_iter()
        .chain(serializer.atom_encoding.into_iter())
        .chain(serializer.structure.bytes.into_iter())
        .collect())
}


struct BitVec {
    bytes: Vec<u8>,
    write_bit: u8,
}

impl BitVec {
    fn new() -> BitVec {
        BitVec {
            bytes: Vec::new(),
            write_bit: 0
        }
    }
    
    fn push(&mut self, value: bool) {
        if self.write_bit == 0 {
            self.bytes.push(0);
        }
        
        if value {
            let last_idx = self.bytes.len()-1;
            self.bytes[last_idx] |= 1 << self.write_bit;
        }
        
        self.write_bit = (self.write_bit + 1) & 7;
    }
    
    //fn bit_len() -> usize {
    //    bytes.len() * 8 - (if self.write_bit == 0 { 0 } else { 8 - write_bit })
    //}
}

#[cfg(test)]
mod test {
    use serialize::serialize;
    use as_noun::AsNoun;
    
    #[test]
    fn serialize_small_byte_atom() {
        assert_eq!( serialize( &5.as_noun(), 100 ),
            Ok([0x01,   0x05,   0x00].to_vec()));
    }
    
    #[test]
    fn serialize_large_byte_atom() {
        assert_eq!( serialize( &190.as_noun(), 100 ),
            Ok([2,   191,190,   0x00].to_vec()));
    }
    
    #[test]
    fn serialize_empty_atom() {
        assert_eq!( serialize( &(&[][..]).as_noun(), 100 ),
            Ok([1,   190,   0x00].to_vec()));
    }
    
    #[test]
    fn serialize_medium_atom() {
        assert_eq!( serialize( &(&[9,8,7,6,5,4,3,2,1,0][..]).as_noun(), 100 ),
            Ok([11,   200,9,8,7,6,5,4,3,2,1,0,   0x00].to_vec()));
    }
    
    fn build_buffer(size: usize) -> Vec<u8> {
        (0..size).map(|idx| (idx*287) as u8).collect()
    }
    
    #[test]
    fn serialize_large_atom() {
        let atom = build_buffer(90);
        assert_eq!( serialize( &(&atom[..]).as_noun(), 100 ),
            Ok([92,   255,90].iter().chain(atom.iter()).chain([0x00].iter()).map(|x| *x).collect()));
    }
    
    #[test]
    fn serialize_larger_atom() {
        let atom = build_buffer(128);
        assert_eq!( serialize( &(&atom[..]).as_noun(), 200 ),
            Ok([131,   255,128,1].iter().chain(atom.iter()).chain([0x00].iter()).map(|x| *x).collect()));
            
        let atom = build_buffer(10922);
        assert_eq!(85*128 + 42, 10922);
        assert_eq!( serialize( &(&atom[..]).as_noun(), 20000 ),
            Ok([192, (10925&0xff) as u8, (10925>>8) as u8,   255,128|42,85].iter().chain(atom.iter()).chain([0x00].iter()).map(|x| *x).collect()));
    }
    
    #[test]
    fn serialize_pair() {
        assert_eq!( serialize( &(50, 60).as_noun(), 100 ),
            Ok([2,   50,60,   0x01].to_vec()));
    }
    
    #[test]
    fn serialize_little_trees() {
        assert_eq!( serialize( &((40, 50), 60).as_noun(), 100 ),
            Ok([3,   40,50,60,   0x03].to_vec()));
        assert_eq!( serialize( &(40, (50, 60)).as_noun(), 100 ),
            Ok([3,   40,50,60,   0x05].to_vec()));
    }
}
