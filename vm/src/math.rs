use noun::{Noun, NounKind};
use eval::{EvalResult, EvalError};
use std::cmp::max;
use std::iter::repeat;

pub fn natural_add(x: &Noun, y: &Noun) -> EvalResult {
    // Fast path: adding two small atoms together
    if let (&Noun::SmallAtom{value:x_value, length: x_length}, &Noun::SmallAtom{value:y_value, length: y_length}) = (x, y) {
        if let Some(sum) = x_value.checked_add(y_value) {
            let required_length =
                if sum <= 0xff {
                    1
                } else if sum <= 0xffff {
                    2
                } else if sum <= 0xffffff {
                    3
                } else {
                    4
                };
            return Ok(Noun::SmallAtom{value: sum, length: max(required_length, max(x_length, y_length))});
        }
    }
    
    // Slow path: byte by byte adding
    let mut x_buf = [0u8; 4];
    let mut y_buf = [0u8; 4];
    if let (NounKind::Atom(xs), NounKind::Atom(ys)) = (x.as_kind(&mut x_buf), y.as_kind(&mut y_buf)) {
        let mut result = Vec::with_capacity(max(xs.len(), ys.len())+1);
        
        // Make xs be the long one
        let (xs, ys) = if xs.len() < ys.len() { (ys, xs) } else { (xs, ys) };
        
        let mut carry: u8 = 0;
        for (x, y) in xs.iter().map(|x| *x).zip(ys.iter().map(|y| *y).chain(repeat(0))) {
            let sum = (x as u16) + (y as u16) + (carry as u16);
            result.push((sum & 0xff) as u8);
            carry = (sum >> 8) as u8;
        }
        if carry != 0 {
            result.push(carry)
        }
        
        return Ok(Noun::from_vec(result));
    }
    
    Err(EvalError::BadArgument)
}

#[cfg(test)]
mod test {
    use math::natural_add;
    use noun::Noun;
    
    #[test]
    fn natural_add_works() {
        assert_eq!(natural_add( &Noun::from_vec(vec![0xff, 0x04]), &Noun::from_u8(2)),
            Ok(Noun::from_vec(vec![0x01, 0x05])));
        
        assert_eq!(natural_add( &Noun::from_vec(vec![0x00, 0x80]), &Noun::from_vec(vec![0x00, 0x80])),
            Ok(Noun::from_vec(vec![0x00, 0x00, 0x01])));
        
        assert_eq!(natural_add( &Noun::from_u8(0xf0), &Noun::from_u8(0x14)),
            Ok(Noun::from_vec(vec![0x04, 0x01])));
    }
}
