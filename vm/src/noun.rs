use std::cmp::{Eq, PartialEq};
use std::rc::Rc;
use std::ops::Deref;
use eval::{EvalError, EvalResult};

#[derive(Clone, Debug)]
pub enum Noun {
    ByteAtom(u8),
    Atom(Rc<Vec<u8>>),
    Cell(Rc<Noun>, Rc<Noun>),
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
    
    pub fn equal(&self, other: &Noun) -> Noun {
        Noun::from_bool(self == other)
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
    
    pub fn axis(&self, index: &Noun) -> EvalResult {
        // LSB first
        match index {
            &Noun::ByteAtom(x) => self.axis_byte(x),
            &Noun::Atom(ref x) => self.axis_bytes(&x),
            &Noun::Cell(_, _) => Err(EvalError::CellAsIndex),
        }
    }
    
    fn axis_bytes(&self, index: &[u8]) -> EvalResult {
        // Find the most significant bit
        let last_nonzero_position = match index.iter().rposition(|b| *b != 0) {
            None => { return Err(EvalError::IndexOutOfRange)}
            Some(pos) => pos
        };
        
        let mut trace: &Noun = self;
        for byte in index[..last_nonzero_position].iter() {
            for bit in 0..8 {
                let go_right = ((*byte) & (1<<bit)) != 0;
                trace = match trace {
                    &Noun::Cell(ref x, ref y) => if go_right { y.deref() } else { x.deref() },
                    _ => { return Err(EvalError::IndexOutOfRange) }
                };
            }
        }
        
        trace.axis_byte(index[last_nonzero_position])
    }
    
    fn axis_byte(&self, mut index: u8) -> EvalResult {
        if index == 0 {
            return Err(EvalError::IndexOutOfRange);
        }
        
        println!("Looking for index {}", index);
        let mut trace = self;
        
        let mut index: u16 = ((index as u16) << 1) | 1;
        
        println!("With a trailing 1, index = {}", index);
        while (index & 0x0100) == 0 {
            index = index << 1;
        }
        
        // Shift out the most significant bit, which has told us which bit position the
        // path starts at but is not part of the path itself.
        index = index << 1; 
        
        while (index & 0x1ff) != 0x0100 {
            let go_right = (index & 0x100) != 0;
            println!("go_right = {}", go_right);
            trace = match trace {
                &Noun::Cell(ref x, ref y) => if go_right { y.deref() } else { x.deref() },
                _ => { return Err(EvalError::IndexOutOfRange) }
            };
            index = index << 1;
        }
        
        Ok(trace.clone())
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
