use noun::Noun;

#[derive(PartialEq, Eq, Debug)]
pub enum DeserializeError {
    UnexpectedEndOfAtomStream,
    UnexpectedEndOfStructureStream,
    OverlongAtom,
    InvalidAtomStreamLength,
    UnexpectedContinuationOfStream,
}

pub type DeserializeResult<T> = Result<T, DeserializeError>;

struct Deserializer<'a> {
    atom_buffer: &'a [u8],
    structure_buffer: &'a [u8],
    structure_bit_pos: u8,
}

impl<'a> Deserializer<'a> {
    fn consume_byte(&mut self) -> DeserializeResult<u8> {
        if let Some(first) = self.atom_buffer.get(0) {
            self.atom_buffer = &self.atom_buffer[1..];
            Ok(*first)
        } else {
            Err(DeserializeError::UnexpectedEndOfAtomStream)
        }
    }
    
    fn consume_structure_bit(&mut self) -> DeserializeResult<bool> {
        if self.structure_buffer.len()==0 {
            return Err(DeserializeError::UnexpectedEndOfStructureStream);
        }
        
        let ret = (self.structure_buffer[0] & (1<<self.structure_bit_pos)) != 0;
        self.structure_bit_pos += 1;
        if self.structure_bit_pos == 8 {
            self.structure_buffer = &self.structure_buffer[1..];
            self.structure_bit_pos = 0;
        }
        
        Ok(ret)
    }

    fn deserialize_atom(&mut self) -> DeserializeResult<Noun> {
        let kind = try!(self.consume_byte());
        
        match kind {
            literal if literal < 190 => {
                Ok(Noun::from_u8(literal))
            }
            x => {
                let length = if x != 255 {
                    x as usize - 190
                } else {
                    let mut length: usize = 0;
                    let mut shift: usize = 0;
                    let mut shift_sentinel: usize = 0x7f;
                    let mut previous_shift_sentinel: usize = 0;
                    // TODO: Once usize's bit count is stabilized, we can use that instead
                    // of this "shift sentinel" business to detect overlong shifts.
                    loop {
                        let b = try!(self.consume_byte());
                        
                        // TODO: Significance is backwards here
                        if (shift_sentinel >> 7) != previous_shift_sentinel {
                            return Err(DeserializeError::OverlongAtom);
                        }
                        length = length | ((b & 0x7f) as usize) << shift;
                        if b < 0x80 {
                            break;
                        }
                        shift += 7;
                        previous_shift_sentinel = shift_sentinel;
                        shift_sentinel = shift_sentinel << 7;
                    }
                    length
                };
                
                if self.atom_buffer.len() < length {
                    return Err(DeserializeError::UnexpectedEndOfAtomStream);
                }
                
                let (atom_bytes, remainder) = self.atom_buffer.split_at(length);
                self.atom_buffer = remainder;
                Ok(Noun::from_slice(atom_bytes))
            }
        }
    }
    
    fn deserialize_noun(&mut self) -> DeserializeResult<Noun> {
        let is_cell = try!(self.consume_structure_bit());
        if is_cell {
            let left = try!(self.deserialize_noun());
            let right = try!(self.deserialize_noun());
            Ok(Noun::new_cell(left, right))
        } else {
            self.deserialize_atom()
        }
    }
    
    fn check_exhausted(&mut self) -> DeserializeResult<()> {
        if self.atom_buffer.len() > 0 {
            return Err(DeserializeError::UnexpectedContinuationOfStream);
        }
        
        if self.structure_buffer.len() > 1 || (self.structure_buffer.len() == 1 && self.structure_bit_pos == 0) {
            return Err(DeserializeError::UnexpectedContinuationOfStream);
        }
        
        Ok( () )
    }
}

pub fn deserialize(buf: &[u8]) -> DeserializeResult<Noun> {
    let mut d = Deserializer{
        atom_buffer: buf,
        structure_buffer: &[],
        structure_bit_pos: 0,
    };
    
    let length = match try!(d.deserialize_atom()).as_usize() {
        Some(length) => length,
        None => { return Err(DeserializeError::InvalidAtomStreamLength); },
    };
    
    if length > d.atom_buffer.len() {
        return Err(DeserializeError::InvalidAtomStreamLength);
    }
    
    let (atoms, structure) = d.atom_buffer.split_at(length);
    
    d = Deserializer{
        atom_buffer: atoms,
        structure_buffer: structure,
        structure_bit_pos: 0,
    };
    
    let result = try!(d.deserialize_noun());
    
    try!(d.check_exhausted());
    
    Ok(result)
}

#[cfg(test)]
mod test {
    use deserialize::deserialize;
    use as_noun::AsNoun;
    use noun::Noun;
    
    #[test]
    fn byte_atom() {
        assert_eq!(deserialize(&[1, 9, 0][..]), Ok(9.as_noun()));
    }
    
    #[test]
    fn large_byte_atom() {
        assert_eq!(deserialize(&[2, 191, 254, 0][..]), Ok(254.as_noun()));
    }
    
    #[test]
    fn a_few_bytes_atom() {
        assert_eq!(deserialize(&[5, 194, 254,253,252,251, 0][..]), Ok((&[254,253,252,251][..]).as_noun()));
    }
    
    #[test]
    fn simple_cell() {
        assert_eq!(deserialize(&[2, 6,7, 1][..]), Ok((6,7).as_noun()));
    }

    fn build_buffer(size: usize) -> Vec<u8> {
        (0..size).map(|idx| (idx*287) as u8).collect()
    }

    #[test]
    fn long_atom() {
        let atom = build_buffer(10922);
        let encoding: Vec<u8> = [192, (10925&0xff) as u8, (10925>>8) as u8,   255,128|42,85].iter().chain(atom.iter()).chain([0x00].iter()).map(|x| *x).collect();
        assert_eq!( deserialize( &encoding[..] ), Ok(Noun::from_vec(atom)));
    }
    
}

