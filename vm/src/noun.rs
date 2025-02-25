use std::convert::TryInto;
use std::cmp::{Eq, PartialEq};
use std::ops::Deref;
use std::rc::Rc;

#[derive(Clone)]
pub enum Noun {
    SmallAtom { value: [u8; 4], length: u8 },
    Atom(Rc<Vec<u8>>),
    Cell(Rc<Noun>, Rc<Noun>),
}

pub enum NounKind<'a> {
    Cell(&'a Noun, &'a Noun),
    Atom(&'a [u8]),
}

impl PartialEq for Noun {
    fn eq(&self, other: &Noun) -> bool {
        match (self, other) {
            (&Noun::Cell(ref a, ref b), &Noun::Cell(ref x, ref y)) => a == x && b == y,
            (
                &Noun::SmallAtom {
                    value: value_a,
                    length: length_a,
                },
                &Noun::SmallAtom {
                    value: value_b,
                    length: length_b,
                },
            ) => (value_a, length_a) == (value_b, length_b),
            (&Noun::Atom(ref a), &Noun::Atom(ref x)) => a == x,
            _ => false, // Nouns that can be SmallAtoms will be SmallAtoms. Doing otherwise would complicate constant-time guarantees.
        }
    }
}
impl Eq for Noun {}

fn own(noun: Rc<Noun>) -> Noun {
    match Rc::try_unwrap(noun) {
        Ok(x) => x,
        Err(shared_x) => shared_x.deref().clone(),
    }
}
fn own_vec(xs: Rc<Vec<u8>>) -> Vec<u8> {
    match Rc::try_unwrap(xs) {
        Ok(x) => x,
        Err(shared_xs) => shared_xs.deref().clone(),
    }
}

impl Noun {
    pub fn new_cell(left: Noun, right: Noun) -> Noun {
        Noun::Cell(Rc::new(left), Rc::new(right))
    }

    pub fn from_bool(source: bool) -> Noun {
        Noun::from_u8(if source { 1 } else { 0 })
    }

    pub fn from_u8(source: u8) -> Noun {
        Noun::SmallAtom {
            value: [source, 0, 0, 0],
            length: 1,
        }
    }

    
    pub fn equal(&self, other: &Noun) -> Noun {
        Noun::from_bool(self == other)
    }

    pub fn from_usize_compact(mut source: usize) -> Noun {
        let mut bs = Vec::new();
        while source != 0 {
            bs.push((source & 0xff) as u8);
            source = source >> 8;
        }
        bs.reverse();
        Noun::from_vec(bs)
    }

    pub fn from_u64_compact(mut source: u64) -> Noun {
        let mut bs = Vec::new();
        while source != 0 {
            bs.push((source & 0xff) as u8);
            source = source >> 8;
        }
        bs.reverse();
        Noun::from_vec(bs)
    }

    pub fn as_usize(&self) -> Option<usize> {
        match self {
            &Noun::Cell(_, _) => None,
            &Noun::SmallAtom { value, length: _ } => {
                u32::from_le_bytes(value).try_into().ok()
            }
            &Noun::Atom(ref xs) => {
                let mut shift = 0u8;
                let mut accum: usize = 0;
                let mut overflow_tester: usize = 0xff;
                for b in xs.iter().map(|b| *b) {
                    if b != 0 {
                        if overflow_tester == 0 {
                            return None; // too big to be a usize
                        }
                        accum = accum | ((b as usize) << shift);
                    }
                    shift += 8;
                    overflow_tester = overflow_tester.overflowing_shl(8).0;
                }
                Some(accum)
            }
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            &Noun::Cell(_, _) => None,
            &Noun::SmallAtom { value, length: _ } => {
                u32::from_le_bytes(value).try_into().ok()
            }
            &Noun::Atom(ref xs) => {
                let mut shift = 0u8;
                let mut accum: u64 = 0;
                let mut overflow_tester: u64 = 0xff;
                for b in xs.iter().map(|b| *b) {
                    if b != 0 {
                        if overflow_tester == 0 {
                            return None; // too big to be a usize
                        }
                        accum = accum | ((b as u64) << shift);
                    }
                    shift += 8;
                    overflow_tester = overflow_tester.overflowing_shl(8).0;
                }
                Some(accum)
            }
        }
    }
    pub fn as_u8(&self) -> Option<u8> {
        match self {
            &Noun::Cell(_, _) => None,
            &Noun::SmallAtom { value, length: _ } => {
                u32::from_le_bytes(value).try_into().ok()
            }
            &Noun::Atom(ref xs) => {
                if xs.len() > 1 {
                    for x in &xs[1..] {
                        if *x != 0 {
                            return None;
                        }
                    }
                }

                xs.get(0).map(|x| *x)
            }
        }
    }

    fn from_small_slice(source: &[u8]) -> Noun {
        Noun::SmallAtom {
            length: source.len() as u8,
            value: [
                source.get(0).map(|x| *x).unwrap_or(0),
                source.get(1).map(|x| *x).unwrap_or(0),
                source.get(2).map(|x| *x).unwrap_or(0),
                source.get(3).map(|x| *x).unwrap_or(0),
            ],
        }
    }

    pub fn from_vec(source: Vec<u8>) -> Noun {
        if source.len() <= 4 {
            Noun::from_small_slice(&source)
        } else {
            Noun::Atom(Rc::new(source))
        }
    }

    pub fn from_slice(source: &[u8]) -> Noun {
        if source.len() <= 4 {
            Noun::from_small_slice(source)
        } else {
            Noun::from_vec(source.to_vec())
        }
    }

    pub fn atom_len(&self) -> Option<usize> {
        match self {
            &Noun::SmallAtom { value: _, length } => Some(length as usize),
            &Noun::Atom(ref xs) => Some(xs.len()),
            &Noun::Cell(_, _) => None,
        }
    }

    pub fn as_kind<'a>(&'a self) -> NounKind<'a> {
        match self {
            &Noun::SmallAtom { ref value, length } => NounKind::Atom(&value[0..length as usize]),
            &Noun::Atom(ref xs) => {
                NounKind::Atom(&xs)
            }
            &Noun::Cell(ref a, ref b) => NounKind::Cell(a, b),
        }
    }
    
    pub fn as_bytes<'a>(&'a self) -> Option<&'a [u8]> {
        match self {
            &Noun::SmallAtom { ref value, length } => Some(&value[0..length as usize]),
            &Noun::Atom(ref xs) => Some(&xs),
            &Noun::Cell(_, _) => None,
        }
    }

    pub fn is_cell(&self) -> bool {
        match self {
            &Noun::Cell(_, _) => true,
            _ => false,
        }
    }

    pub fn as_cell(&self) -> Option<(&Noun, &Noun)> {
        match self {
            &Noun::Cell(ref a, ref b) => Some((a, b)),
            _ => None,
        }
    }

    pub fn into_cell(self) -> Option<(Noun, Noun)> {
        match self {
            Noun::Cell(a, b) => Some((own(a), own(b))),
            _ => None,
        }
    }

    pub fn as_byte(&self) -> Option<u8> {
        match self {
            &Noun::SmallAtom { value, length: _ } => {
                if value[1] == 0 && value[2] == 0 && value[3] == 0 {
                    Some(value[0])
                } else {
                    None
                }
            }
            &Noun::Atom(ref xs) => {
                if (&xs[1..]).iter().position(|x| *x != 0).is_some() {
                    return None;
                }

                Some(xs.get(0).map(|x| *x).unwrap_or(0))
            }
            _ => None,
        }
    }

    pub fn into_vec(self) -> Option<Vec<u8>> {
        match self {
            Noun::SmallAtom { value, length } => Some(value[0..length as usize].to_vec()),
            Noun::Atom(xs) => Some(own_vec(xs)),
            Noun::Cell(_, _) => None,
        }
    }
}

impl ::std::fmt::Debug for Noun {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> Result<(), ::std::fmt::Error> {
        match self {
            &Noun::Cell(ref a, ref b) => write!(f, "[{:?} {:?}]", a, b),
            &Noun::SmallAtom { value, length } => {
                if length == 1 { // Displaying opcodes as decimal makes for cleaner rendering of code
                    return write!(f, "{}", value[0]);
                }
                
                f.write_str("x")?;
                
                for byte in value[..length as usize].iter() {
                    write!(f, "{:02x}", *byte)?;
                }
                Ok(())
            }
            &Noun::Atom(ref a) => {
                if a.len() == 1 { // Displaying opcodes as decimal makes for cleaner rendering of code
                    return write!(f, "{}", a[0]);
                }
                
                f.write_str("x")?;
                
                for byte in a.iter() {
                    write!(f, "{:02x}", *byte)?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod test {
    use as_noun::AsNoun;
    //use noun::Noun;

    #[test]
    fn eq() {
        assert_eq!((1, 2).as_noun(), (1, 2).as_noun());
    }
}
