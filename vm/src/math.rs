use noun::Noun;
use std::cmp::min;

pub fn add(x: &Noun, y: &Noun) -> Option<Noun> {
    let x_bytes = x.as_bytes()?;
    let y_bytes = y.as_bytes()?;
    
    let (long, short) = if x_bytes.len() > y_bytes.len() { (x_bytes, y_bytes) } else { (y_bytes, x_bytes) };
    
    let mut ret = long.to_vec();
    let mut carry = 0;
    let (paired, unpaired) = ret.split_at_mut(short.len());
    
    for (x, y) in paired.iter_mut().zip(short.iter()) {
        let z = (*x as u16) + (*y as u16) + carry;
        *x = z as u8;
        carry = z >> 8;
    }
    
    for x in unpaired.iter_mut() {
        let z = (*x as u16) + carry;
        *x = z as u8;
        carry = z >> 8;
    }
    
    Some(Noun::from_vec(ret))
}

pub fn invert(x: &Noun) -> Option<Noun> {
    let xs = x.as_bytes()?;
    Some(Noun::from_vec(xs.iter().map(|x| !x).collect()))
}

pub fn xor(x: &Noun, y: &Noun) -> Option<Noun> {
    let x_bytes = x.as_bytes()?;
    let y_bytes = y.as_bytes()?;
    
    let (long, short) = if x_bytes.len() > y_bytes.len() { (x_bytes, y_bytes) } else { (y_bytes, x_bytes) };
    let (paired, unpaired) = long.split_at(short.len());
    
    Some(Noun::from_vec(paired.iter().zip(short.iter()).map(|(x, y)| *x ^ *y).chain(unpaired.iter().map(|x| *x)).collect()))
}

pub fn less(x: &Noun, y: &Noun) -> Option<bool> {
    let x_bytes = x.as_bytes()?;
    let y_bytes = y.as_bytes()?;

    let common_len = min(x_bytes.len(), y_bytes.len());
    let (x_prefix, x_suffix) = x_bytes.split_at(common_len);
    let (y_prefix, y_suffix) = y_bytes.split_at(common_len);
    
    let mut overall_lesser = false;
    for (x, y) in x_prefix.iter().zip(y_prefix.iter()) {
        overall_lesser = (overall_lesser & (*x <= *y)) | (*x < *y);
    }
    for x in x_suffix {
        overall_lesser = overall_lesser & (*x == 0);
    }
    for y in y_suffix {
        overall_lesser = overall_lesser | (*y != 0);
    }
    return Some(overall_lesser);
}

#[test]
fn less_cases() {
    assert_eq!(less(&Noun::from_usize_compact(4), &Noun::from_usize_compact(8)), Some(true));
    assert_eq!(less(&Noun::from_usize_compact(14), &Noun::from_usize_compact(14)), Some(false));
    assert_eq!(less(&Noun::from_usize_compact(30), &Noun::from_usize_compact(5)), Some(false));
    assert_eq!(less(&Noun::from_usize_compact(30), &Noun::from_usize_compact(0x100)), Some(true));
    assert_eq!(less(&Noun::from_usize_compact(0x1234), &Noun::from_usize_compact(0x56)), Some(false));
}