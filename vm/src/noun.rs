use std::cmp::{Eq, PartialEq};
use std::rc::Rc;
use checked_int_cast::CheckedIntCast;

#[derive(Clone, Debug)]
pub enum Noun {
    SmallAtom{value: u32, length: u8},
    Atom(Rc<Vec<u8>>),
    Cell(Rc<Noun>, Rc<Noun>),
}

pub enum NounKind<'a> {
    Cell(&'a Noun, &'a Noun),
    Atom(&'a [u8]),
}

fn as_truncated_u32(bs: &[u8]) -> u32 {
    (bs.get(0).map(|x| *x).unwrap_or(0) as u32)
    | ((bs.get(1).map(|x| *x).unwrap_or(0) as u32) << 8)
    | ((bs.get(2).map(|x| *x).unwrap_or(0) as u32) << 16)
    | ((bs.get(3).map(|x| *x).unwrap_or(0) as u32) << 24)
}

impl PartialEq for Noun {
    fn eq(&self, other: &Noun) -> bool {
        match (self, other) {
            (&Noun::Cell(ref a, ref b), &Noun::Cell(ref x, ref y)) => a==x && b==y,
            (&Noun::SmallAtom{value:value_a, length:length_a}, &Noun::SmallAtom{value:value_b, length:length_b}) => (value_a,length_a)==(value_b,length_b),
            (&Noun::Atom(ref a), &Noun::Atom(ref x)) => a==x,
            (&Noun::SmallAtom{value:value_a, length:length_a}, &Noun::Atom(ref b)) => b.len()==(length_a as usize) && as_truncated_u32(b) == value_a,
            (&Noun::Atom(ref a), &Noun::SmallAtom{value:value_b, length:length_b}) => a.len()==(length_b as usize) && as_truncated_u32(a) == value_b,
            _ => false
        }
    }
}
impl Eq for Noun {}



impl Noun {
    pub fn new_cell(left: Noun, right: Noun) -> Noun {
        Noun::Cell(Rc::new(left), Rc::new(right))
    }
    
    pub fn from_bool(source: bool) -> Noun {
        Noun::from_u8(if source { 0 } else { 1 })
    }
    
    pub fn from_u8(source: u8) -> Noun {
        Noun::SmallAtom{value: source as u32, length: 1}
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
        Noun::from_vec(bs)
    }
    
    pub fn as_usize(&self) -> Option<usize> {
        match self {
            &Noun::Cell(_, _) => None,
            &Noun::SmallAtom{value, length: _} => value.as_usize_checked(),
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
                    overflow_tester = overflow_tester << 8;
                }
                Some(accum)
            }
        }
    }
    
    pub fn from_vec(source: Vec<u8>) -> Noun {
        Noun::Atom(Rc::new(source))
    }
    
    pub fn from_slice(source: &[u8]) -> Noun {
        Noun::from_vec(source.to_vec())
    }
    
    pub fn as_kind<'a>(&'a self, buf: &'a mut [u8; 4]) -> NounKind<'a> {
        match self {
            &Noun::SmallAtom{value, length} => {
                // TODO: Use byteorder
                buf[0] = value as u8;
                buf[1] = (value>>8) as u8;
                buf[2] = (value>>16) as u8;
                buf[3] = (value>>24) as u8;
                NounKind::Atom(&buf[0..length as usize])
            }
            &Noun::Atom(ref xs) => {
                //let ys: &'a Rc<Vec<u8>> = xs;
                NounKind::Atom(&xs)
            }
            &Noun::Cell(ref a, ref b) => {
                NounKind::Cell(a, b)
            }
        }
    }
    
    pub fn as_byte(&self) -> Option<u8> {
        match self {
            &Noun::SmallAtom{value, length: _} => {
                if value<256 {
                    Some(value as u8)
                } else {
                    None
                }
            }
            &Noun::Atom(ref xs) => {
                if (&xs[1..]).iter().position(|x| *x!=0).is_some() {
                    return None;
                }
                
                Some(xs.get(0).map(|x| *x).unwrap_or(0))
            }
            _ => {
                None
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
