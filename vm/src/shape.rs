use noun::{Noun, NounKind};
use std::io::{self, Read};
use ticks::Ticks;
use std::mem::size_of;

#[derive(Debug, Eq, PartialEq)]
pub enum ShapeError {
    AllocationBoundExceeded,
    DataTooShort,
}

fn populate_structure<R: Read>(structure: &Noun, data_source: &mut R, allocation_bound: &mut Ticks) -> Result<Noun, ShapeError> {
    try!(allocation_bound.incur(size_of::<Noun>() as u64).map_err(|_| ShapeError::AllocationBoundExceeded));

    if let Some((left, right)) = structure.as_cell() {
        return Ok(Noun::new_cell(
            try!(populate_structure(left, data_source, allocation_bound)),
            try!(populate_structure(right, data_source, allocation_bound)) ));
    }

    let expected_count = try!(structure.as_usize().ok_or(ShapeError::AllocationBoundExceeded));
    try!(allocation_bound.incur(expected_count as u64).map_err(|_| ShapeError::AllocationBoundExceeded));

    let mut xs = vec![0u8; expected_count];
    try!(data_source.read_exact(&mut xs[..]).map_err(|_| ShapeError::DataTooShort));

    Ok(Noun::from_vec(xs))
}

pub struct NounReader<'a> {
    current_node: Option<(&'a Noun, usize)>,
    stack: Vec<&'a Noun>,
    ticks: &'a mut Ticks,
}

impl<'a> NounReader<'a> {
    fn new(noun: &'a Noun, ticks: &'a mut Ticks) -> NounReader<'a> {
        NounReader {
            current_node: None,
            stack: vec![noun],
            ticks: ticks,
        }
    }
}

impl<'a> Read for NounReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.current_node.is_none() {
            let mut stack_top: &'a Noun = if let Some(stack_top) = self.stack.pop() {
                    stack_top
                } else {
                    return Ok(0);
                };

            loop {
                if let Some((left, right)) = stack_top.as_cell() {
                    stack_top = left;
                    self.stack.push(right);
                } else if stack_top.atom_len().unwrap_or(0) > 0 {
                    self.current_node = Some((stack_top, 0));
                    break;
                } else {
                    break;
                }
            }
        }

        let mut finished = false;
        let ret = if let Some((ref mut current_node, ref mut pos)) = self.current_node {
            Ok(match current_node.as_kind() {
                NounKind::Atom(noun_contents) => {
                    let read_count = try!((&noun_contents[*pos..]).read(buf));
                    *pos += read_count;
                    finished = *pos == noun_contents.len();

                    try!(self.ticks.incur(read_count as u64).map_err(|_| io::Error::new(io::ErrorKind::Interrupted, "cost exceeded")));

                    read_count
                }
                _ => {
                    panic!("NounReader's current_node was not an atom")
                }
            })
        } else {
            Ok(0)
        };

        if finished {
            self.current_node = None;
        }

        ret
    }
}

pub fn shape(data: &Noun, structure: &Noun, ticks: &mut Ticks, allocation_bound: usize) -> Result<Noun, ShapeError> {
    populate_structure(structure, &mut NounReader::new(data, ticks), &mut Ticks::new(allocation_bound as u64))
}

#[cfg(test)]
mod test {
    use as_noun::AsNoun;
    use super::{shape, ShapeError};
    use ticks::Ticks;
    use noun::Noun;

    fn testcase<D: AsNoun, S: AsNoun, R: AsNoun>(data: D, structure: S, expected_result: R) {
        assert_eq!(
            shape(&data.as_noun(), &structure.as_noun(), &mut Ticks::new(1_000_000), 1_000_000),
            Ok(expected_result.as_noun())
        )
    }

    fn is_malformed<D: AsNoun, S: AsNoun>(data: D, structure: S, error: ShapeError) {
        assert_eq!(shape(&data.as_noun(), &structure.as_noun(), &mut Ticks::new(1_000_000), 1_000_000), Err(error));
    }

    #[test]
    fn cut() {
        testcase(&[1,2,3,4,5][..], (2, 3), (&[1,2][..], &[3,4,5][..]));
    }

    #[test]
    fn join() {
        testcase((&[1,2][..], &[3,4,5][..]), 5, &[1,2,3,4,5][..]);
    }

    #[test]
    fn join_with_empty() {
        testcase((&[1,2][..], &[][..], &[3,4,5][..], &[][..]), 5, &[1,2,3,4,5][..]);
    }

    #[test]
    fn rearrange() {
        testcase((&[1,2][..], &[3,4,5][..]), (3, 2), (&[1,2,3][..], &[4,5][..]));
    }

    #[test]
    fn too_short_input() {
        is_malformed((&[1,2][..], &[3,4,5][..]), 6, ShapeError::DataTooShort);
    }

    #[test]
    fn too_long() {
        let mut x = Noun::from_u8(1);
        for _ in 0..50 {
            x = Noun::new_cell(x.clone(), x.clone());
        }

        is_malformed(x, Noun::from_usize_compact(2_000_000), ShapeError::AllocationBoundExceeded);
    }
}
