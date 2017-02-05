use noun::{Noun, NounKind};
use std::io::{self, Read};
use ticks::{Ticks, CostResult};

fn populate_structure<R: Read>(structure: &Noun, data_source: &mut R) -> Noun {
    if let Some((left, right)) = structure.as_cell() {
        return Noun::new_cell( populate_structure(left, data_source), populate_structure(right, data_source) );
    }

    let expected_count = structure.as_usize().expect("populate_structure had a structure atom that was not a size");
    let mut xs = vec![0u8; expected_count];
    data_source.read_exact(&mut xs[..]).expect("populate_structure data source exhausted");
    Noun::from_vec(xs)
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

        let mut little_buffer = [0u8; 4];
        let mut finished = false;
        let ret = if let Some((ref mut current_node, ref mut pos)) = self.current_node {
            Ok(match current_node.as_kind(&mut little_buffer) {
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

pub fn shape(data: &Noun, structure: &Noun, ticks: &mut Ticks) -> CostResult<Option<Noun>> {
    let mut nr = NounReader::new(data, ticks);

    Ok(Some(populate_structure(structure, &mut nr)))
}

#[cfg(test)]
mod test {
    use as_noun::AsNoun;
    use super::shape;
    use ticks::Ticks;

    fn testcase<D: AsNoun, S: AsNoun, R: AsNoun>(data: D, structure: S, expected_result: R) {
        assert_eq!(
            shape(&data.as_noun(), &structure.as_noun(), &mut Ticks::new(1_000_000)),
            Ok(Some(expected_result.as_noun()))
        )
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
}
