use std::cmp::{Eq, PartialEq};
use std::rc::Rc;

#[derive(Clone, Debug)]
pub enum Noun {
    ByteAtom(u8),
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
            (&Noun::Cell(ref a, ref b), &Noun::Cell(ref x, ref y)) => a==x && b==y,
            (&Noun::ByteAtom(a), &Noun::ByteAtom(x)) => a==x,
            (&Noun::Atom(ref a), &Noun::Atom(ref x)) => a==x,
            (&Noun::ByteAtom(a), &Noun::Atom(ref x)) => x.len()==1 && a == x[0],
            (&Noun::Atom(ref a), &Noun::ByteAtom(x)) => a.len()==1 && a[0] == x,
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
        Noun::ByteAtom(if source { 0 } else { 1 })
    }
    
    pub fn from_u8(source: u8) -> Noun {
        Noun::ByteAtom(source)
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
            &Noun::ByteAtom(x) => Some(x as usize),
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
            &Noun::ByteAtom(x) => {
                buf[0] = x;
                NounKind::Atom(&buf[0..1])
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
            &Noun::ByteAtom(x) => { Some(x) }
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
