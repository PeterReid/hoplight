use noun::Noun;
use eval::{EvalError, EvalResult};
use std::ops::Deref;

pub trait Axis {
    fn axis(&self, index: &Noun) -> EvalResult;
}

struct ByteBitIterator {
    data: u16
}

impl ByteBitIterator {
    fn new(data: u8) -> ByteBitIterator {
        ByteBitIterator{
            data: ((data as u16)<<1) | 1
        }
    }
    
    fn empty() -> ByteBitIterator {
        ByteBitIterator{ data: 0 }
    }
}

impl Iterator for ByteBitIterator {
    type Item = bool;
    
    fn next(&mut self) -> Option<bool> {
        if (self.data & 0xff) == 0 {
            return None;
        }
        
        let bit = (self.data & 0x100) != 0;
        self.data = self.data << 1;
        Some(bit)
    }
}

struct ByteSliceBitIterator<'a> {
    inner: &'a [u8],
    current_byte: ByteBitIterator,
}

impl<'a> ByteSliceBitIterator<'a> {
    fn new(bs: &[u8]) -> ByteSliceBitIterator {
        if bs.len() > 0 {
            ByteSliceBitIterator{
                inner: &bs[0..bs.len()-1],
                current_byte: ByteBitIterator::new(bs[bs.len()-1]),
            }
        } else {
            ByteSliceBitIterator{
                inner: bs,
                current_byte: ByteBitIterator::empty(),
            }
        }
    }
}

impl<'a> Iterator for ByteSliceBitIterator<'a> {
    type Item = bool;
    
    fn next(&mut self) -> Option<bool> {
        if let Some(bit) = self.current_byte.next() {
            return Some(bit);
        }
        
        self.current_byte = match self.inner.last() {
            None => { return None },
            Some(byte) => ByteBitIterator::new(*byte)
        };
        self.inner = &self.inner[0..self.inner.len()-1];
        
        self.current_byte.next()
    }
}

fn axis_for<T: Iterator<Item=bool>>(subject: &Noun, mut bits: T) -> EvalResult {
    // The most significant set bits tells us where to start moving left and right.
    loop {
        match bits.next() {
            None => {
                return Result::Err(EvalError::IndexOutOfRange);
            },
            Some(true) => {
                break;
            }
            Some(false) => { }
        }
    }
    
    // The remaining bits, most significant first, tell us whether to go left or right.
    let mut trace = subject;
    for go_right in bits {
        trace = match trace {
            &Noun::Cell(ref x, ref y) => if go_right { y.deref() } else { x.deref() },
            _ => { return Err(EvalError::IndexOutOfRange) }
        };
    }
    
    Ok(trace.clone())
}

impl Axis for Noun {
    fn axis(&self, index: &Noun) -> EvalResult {
        match index {
            &Noun::ByteAtom(x) => axis_for(self, ByteBitIterator::new(x)),
            &Noun::Atom(ref xs) => axis_for(self, ByteSliceBitIterator::new(xs)),
            &Noun::Cell(_, _) => Err(EvalError::CellAsIndex),
        }
    }
}
