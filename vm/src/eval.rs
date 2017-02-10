use noun::{Noun};
use axis::Axis;
use math;
use crypto::blake2b::Blake2b;
use serialize::{self, SerializationError};
use deserialize::deserialize;
use opcode::*;
use shape::shape;
use std::convert::From;
use ticks::{CostError, Ticks};
use equal::equal;

#[derive(Debug, Eq, PartialEq)]
pub enum EvalError {
    Something,
    CellAsIndex,
    IndexOutOfRange,
    InvalidLength,
    NotAnOpcode,
    BadOpcode(u8),
    BadRecurseArgument,
    BadEqualsArgument,
    BadArgument,
    BadIfCondition,
    TickLimitExceeded,
    AtomicFormula,
    MemoryExceeded,
    StorageCorrupt,
    EvalOnAtom,
    BadShape,
}

fn into_triple(noun: Noun) -> Option<(Noun, Noun, Noun)> {
    if let Some((a, bc)) = noun.into_cell() {
        if let Some((b, c)) = bc.into_cell() {
            return Some((a, b, c));
        }
    }
    None
}

pub type EvalResult = Result<Noun, EvalError>;

pub trait SideEffectEngine {
    fn nearest_neighbor(&mut self, near: &[u8; 32]) -> [u8; 32];
    fn random(&mut self, &mut [u8]);
    fn load(&mut self, key: &[u8]) -> Option<Vec<u8>>;
    fn store(&mut self, key: &[u8], value: &[u8]);
    fn send(&mut self, destination: &[u8; 32], message: &[u8], local_cost: u64);
}

struct Computation<'a, S: 'a> {
    ticks_remaining: Ticks,
    side_effector: &'a mut S,
}

impl From<CostError> for EvalError {
    fn from(_: CostError) -> EvalError {
        EvalError::TickLimitExceeded
    }
}

impl<'a, S: SideEffectEngine> Computation<'a, S> {
    pub fn retrieve_with_tag(&mut self, subject: Noun, mut key: Vec<u8>, tag: u8) -> Result<Noun, EvalError> {
        key.push(tag);

        // TODO: It might be better to always return a cell.
        if let Some(xs) = self.side_effector.load(&key[..]) {
            let retrieved = try!(deserialize(&xs[..]).map_err(|_| EvalError::StorageCorrupt));
            Ok(Noun::new_cell(
                Noun::from_bool(true),
                try!(self.eval_on(subject, retrieved))
            ))
        } else {
            Ok(Noun::from_bool(false))
        }
    }

    pub fn eval_on(&mut self, mut subject: Noun, mut formula: Noun) -> EvalResult {
        'tail_recurse: loop {
            try!(self.ticks_remaining.incur(1));

            let (opcode_noun, argument) = try!(formula.into_cell().ok_or(EvalError::AtomicFormula));
            if opcode_noun.is_cell() {
                // Distribute. The opcode and argument are actually both formulas.
                let lhs = try!(self.eval_on(subject.clone(), opcode_noun));
                let rhs = try!(self.eval_on(subject, argument));
                return Ok(Noun::new_cell(lhs, rhs));
            }

            let opcode = try!(opcode_noun.as_u8().ok_or(EvalError::NotAnOpcode));

            return match opcode {
                AXIS => subject.axis(&argument),
                LITERAL => Ok(argument),
                RECURSE => {
                    if let Some((b, c)) = argument.into_cell() {
                        let b_result = try!(self.eval_on(subject.clone(), b));
                        let c_result = try!(self.eval_on(subject, c));
                        subject = b_result;
                        formula = c_result;
                        continue 'tail_recurse;
                    } else {
                        Err(EvalError::BadRecurseArgument)
                    }
                }
                IS_CELL => { // cell test
                    Ok(Noun::from_bool(try!(self.eval_on(subject, argument)).is_cell()))
                }
                INCREMENT => { // increment
                    math::natural_add(&try!(self.eval_on(subject, argument)), &Noun::from_u8(1))
                }
                IS_EQUAL => {
                    if let Some((lhs, rhs)) = try!(self.eval_on(subject, argument)).as_cell() {
                        Ok(Noun::from_bool(equal(lhs, rhs, &mut self.ticks_remaining)?))
                    } else {
                        Err(EvalError::BadEqualsArgument)
                    }
                }
                IF => {
                    if let Some((b, c, d)) = into_triple(argument) {
                        let condition = try!(self.eval_on(subject.clone(), b));
                        match condition.as_u8() {
                            Some(0) => {
                                formula = c;
                                continue 'tail_recurse;
                            }
                            Some(1) => {
                                formula = d;
                                continue 'tail_recurse;
                            }
                            _ => {
                                Err(EvalError::BadIfCondition)
                            }
                        }
                    } else {
                        Err(EvalError::BadArgument)
                    }
                }
                COMPOSE => {
                    if let Some((b, c)) = argument.into_cell() {
                        let b_of_x = try!(self.eval_on(subject, b));
                        subject = b_of_x;
                        formula = c;
                        continue 'tail_recurse;
                    } else {
                        Err(EvalError::BadArgument)
                    }
                }
                DEFINE => {
                    if let Some((b, c)) = argument.into_cell() {
                        let subject_prime = try!(self.eval_on(subject.clone(), b));
                        subject = Noun::new_cell(subject_prime, subject);
                        formula = c;
                        continue 'tail_recurse;
                    } else {
                        Err(EvalError::BadArgument)
                    }
                }
                CALL => {
                    if let Some((b, c)) = argument.into_cell() {
                        let core = try!(self.eval_on(subject, c));
                        let inner_formula = try!(core.axis(&b));
                        subject = core;
                        formula = inner_formula;
                        continue 'tail_recurse;
                    } else {
                        Err(EvalError::BadArgument)
                    }
                }
                HASH => { // hash
                    let hash_target = try!(self.eval_on(subject, argument));
                    let buffer = try!(self.serialize(hash_target));
                    try!(self.ticks_remaining.incur(20 + (buffer.len() as u64)));
                    let mut result = [0u8; 64];
                    Blake2b::blake2b(&mut result[..], &buffer, &[][..]);
                    Ok(Noun::from_slice(&result[..]))
                }
                STORE_BY_HASH => { // store by hash
                    let hash_target = try!(self.eval_on(subject, argument));
                    let buffer = try!(self.serialize(hash_target));
                    try!(self.ticks_remaining.incur(20 + (buffer.len() as u64)));
                    let mut result = [0u8; 64 + 1];
                    result[64] = 1;
                    Blake2b::blake2b(&mut result[..64], &buffer, &[][..]);
                    self.side_effector.store(&result[..], &buffer[..]);
                    Ok(Noun::from_bool(true)) // TODO: It might be better to return the hash
                }
                RETRIEVE_BY_HASH => { // retrieve by hash
                    let hash = try!(self.eval_on(subject.clone(), argument));
                    if let Some(hash_bytes) = hash.into_vec() {
                        self.retrieve_with_tag(subject, hash_bytes, 1)
                    } else {
                        Ok(Noun::from_bool(false))
                    }
                }
                STORE_BY_KEY => {
                    if let Some((key, value)) = self.eval_on(subject, argument)?.into_cell() {
                        let mut storage_key = try!(self.serialize(key));
                        storage_key.push(0);
                        let storage_value = try!(self.serialize(value));
                        self.side_effector.store(&storage_key[..], &storage_value[..]);
                        Ok(Noun::from_bool(true))
                    } else {
                        Err(EvalError::BadArgument)
                    }
                }
                RETRIEVE_BY_KEY => {
                    let key = try!(self.eval_on(subject.clone(), argument));
                    let key_bytes = try!(self.serialize(key));
                    self.retrieve_with_tag(subject, key_bytes, 0)
                }
                RANDOM => {
                    let length = try!(try!(self.eval_on(subject, argument)).as_usize().ok_or(EvalError::InvalidLength));
                    if length > 1_000_000 {
                        return Err(EvalError::InvalidLength);
                    }
                    let mut xs = vec![0u8; length];
                    self.side_effector.random(&mut xs);
                    Ok(Noun::from_vec(xs))
                }
                SHAPE => {
                    if let Some((data, structure)) = self.eval_on(subject, argument)?.into_cell() {
                        shape(
                            &data,
                            &structure,
                            &mut self.ticks_remaining,
                            10_000_000,
                        ).map_err(|_| EvalError::BadShape)
                    } else {
                        Err(EvalError::BadArgument)
                    }
                }
                //11 => { // send
                //    if let Some((b, c, d)) =
                //}

                _ => Err(EvalError::BadOpcode(opcode)),
            };
        }
    }

    fn serialize(&mut self, noun: Noun) -> Result<Vec<u8>, EvalError> {
        match serialize::serialize(&noun, 1_000_000) {
            Ok(x) => Ok(x),
            Err(SerializationError::OverlongAtom) => Err(EvalError::BadArgument),
            Err(SerializationError::MaximumLengthExceeded) => Err(EvalError::MemoryExceeded),
        }
    }
}


pub fn eval<S: SideEffectEngine>(expression: Noun, side_effector: &mut S, tick_limit: u64) -> EvalResult {
    if let Some((subject, formula)) = expression.into_cell() {
        Computation{
            ticks_remaining: Ticks::new(tick_limit),
            side_effector: side_effector,
        }.eval_on(subject, formula)
    } else {
        Err(EvalError::EvalOnAtom)
    }
}

#[cfg(test)]
mod test {
    use noun::Noun;
    use as_noun::AsNoun;
    use eval::{eval, SideEffectEngine};
    use std::collections::HashMap;
    use opcode::*;
    use chacha::{ChaCha, KeyStream};

    struct TestSideEffectEngine {
        storage: HashMap<Vec<u8>, Vec<u8>>,
        rng: ChaCha,
    }

    impl TestSideEffectEngine {
        fn new() -> TestSideEffectEngine {
            TestSideEffectEngine{
                storage: HashMap::new(),
                rng: ChaCha::new_chacha20(&[1u8; 32], &[0u8; 8]),
            }
        }
    }

    impl SideEffectEngine for TestSideEffectEngine {
        fn nearest_neighbor(&mut self, _near: &[u8; 32]) -> [u8; 32] {
            [0u8; 32]
        }
        fn random(&mut self, dest: &mut [u8]) {
            for b in dest.iter_mut() {
                *b = 0;
            }
            self.rng.xor_read(dest).expect("RNG end reached");
        }
        //fn expected_storage_return(&mut self, storage_signing_key: &[u8; 32]) -> u64 { // guess of how much it will pay
        //    0
        //}
        fn load(&mut self, key: &[u8]) -> Option<Vec<u8>> {
            self.storage.get(key).cloned()
        }
        fn store(&mut self, key: &[u8], value: &[u8]) {
            self.storage.insert(key.into(), value.into());
        }
        fn send(&mut self, _destination: &[u8; 32], _message: &[u8], _local_cost: u64) {
        }
    }

    fn eval_simple<E: AsNoun>(expression: E) -> Noun {
        let mut engine = TestSideEffectEngine::new();
        eval(expression.as_noun(), &mut engine, 1000000).expect("eval_simple expression got an error")
    }

    fn expect_eval_with<E: AsNoun, R: AsNoun>(engine: &mut TestSideEffectEngine, expression: E, result: R) {
        assert_eq!(
            eval(expression.as_noun(), engine, 1000000),
            Ok( result.as_noun() )
        );
    }

    fn expect_eval<E: AsNoun, R: AsNoun>(expression: E, result: R) -> TestSideEffectEngine {
        let mut engine = TestSideEffectEngine::new();
        expect_eval_with(&mut engine, expression, result);
        engine
    }

    #[test]
    fn literal_op() {
        expect_eval((0, 1, 44), 44);

        expect_eval(
            ((76, 30), 1, (42, 60)),
            (42, 60)
        );
    }

    #[test]
    fn axis_op() {
        expect_eval((99, 0, 1), 99);
        expect_eval(((98, 99), 0, 2), 98);
        expect_eval(((98, 99), 0, 3), 99);

        expect_eval(((1, 2, 3, 4, (5, 6, 7, (8, 9, 10, 11))), 0, &[0xff, 0x07][..]),
            11);
        expect_eval(((((1, 2), 3), 4), 0, 5),
            3);
        expect_eval(((((1, 2), 3), 4), 0, 4),
            (1, 2));
    }

    #[test]
    fn recurse_op() {
        expect_eval(((123, (0, 1)), 2, (0, 2), (0, 3)), 123);
    }

    #[test]
    fn equal_op() {
        expect_eval(((5, 5), 5, (0, 1)),  Noun::from_bool(true));
        expect_eval(((5, 8), 5, (0, 1)),  Noun::from_bool(false));
    }

    #[test]
    fn cell_op() {
        expect_eval(((99, 33), 3, (0, 1)),  Noun::from_bool(true));
        expect_eval((99, 3, (0, 1)),  Noun::from_bool(false));
    }

    #[test]
    fn increment_op() {
        expect_eval((22, 4, (0, 1)),  Noun::from_u8(23));
        expect_eval((0xff, 4, (0, 1)),  Noun::from_vec(vec![0x00, 0x01]));
    }

    #[test]
    fn distribute() {
        expect_eval(
            (22, (4, (0, 1)), (0, 1), (1, 50)),
            (23, 22, 50).as_noun()
        );
    }

    #[test]
    fn if_true() {
        expect_eval(
            (42, (6, (1, 0), (4, 0, 1), (1, 233))),
            43);
    }

    #[test]
    fn if_false() {
        expect_eval(
            (42, (6, (1, 1), (4, 0, 1), (1, 233))),
            233);
    }

    #[test]
    fn composition() {
        expect_eval(
            (42, (7, (4, 0, 1), (4, 0, 1))),
            44);
    }

    #[test]
    fn push_1() {
        expect_eval(
            (42, (8, (4, 0, 1), (0, 1))),
            (43, 42));
    }

    #[test]
    fn push_2() {
        expect_eval(
            (42, (8, (4, 0, 1), (4, 0, 3))),
            43);
    }

    #[test]
    fn decrement() {
        expect_eval(
            (42, (8, (1, 0), 8, (1, 6, (5, (0, 7), 4, 0, 6), (0, 6), (9, 2, (0, 2), (4, 0, 6), 0, 7)), (9, 2, 0, 1))),
            41);
    }

    #[test]
    fn store_and_get_hash() {
        let mut engine = expect_eval(
            (21, RECURSE, ((STORE_BY_HASH, ((LITERAL, LITERAL), (AXIS, 1))), (INCREMENT, (AXIS, 1))),   (LITERAL, AXIS, 3)),
            22
            );
        let hash = eval_simple( (21, HASH, ((LITERAL, LITERAL), (AXIS, 1))) );
        expect_eval_with(&mut engine,
            (hash, (RETRIEVE_BY_HASH, (AXIS, 1))),
            (0, 21));
    }

    #[test]
    fn store_and_get_key() {
        let mut engine = expect_eval(
            (&b"orange"[..], (STORE_BY_KEY, (LITERAL, &b"color"[..]), ((LITERAL, LITERAL), (AXIS, 1)))),
            Noun::from_bool(true));
        expect_eval_with(&mut engine,
            (&b"color"[..], (RETRIEVE_BY_KEY, (AXIS, 1))),
            (0, &b"orange"[..]));
    }

    #[test]
    fn gen_random() {
        let random = eval_simple( (20, RANDOM, (AXIS, 1)) );
        let buf = random.into_vec().expect("random should have made an atom");
        assert_eq!(buf.len(), 20);
        // These bytes will probably be different.
        // We use a deterministic RNG, so a this failure won't be intermittent.
        // I do want to catch leaving this uninitialized somehow.
        assert!(buf[0] != buf[1] || buf[1] != buf[2]);
    }
    
    #[test]
    fn guessing_game() {
        let rightleftleft = 12;
        let rightleftright = 13;
        let rightright = 7;
        let left = 2;
        let rightleft = 6;
        
        let f = (IF, (IS_EQUAL, (AXIS, rightleftleft), (AXIS, rightleftright)),
            (LITERAL, &b"correct"[..]),
            (IF, (IS_EQUAL, (AXIS, rightleftleft), (AXIS, rightright)),
                (LITERAL, &b"too small"[..]),
                (IF, (IS_EQUAL, (AXIS, rightleftright), (AXIS, rightright)),
                    (LITERAL, &b"too big"[..]),
                    (RECURSE, ((AXIS, left),
                              ((AXIS, rightleft),
                               (INCREMENT, (AXIS, rightright))
                              )),
                              (AXIS, left)
                    )))).as_noun();
        let make_context_and_data = (
            (LITERAL, f.clone()),
            (
              (
                (AXIS, 1),
                (LITERAL, 42),
              ),
              (LITERAL, 0)
            )
        ).as_noun();
        
        let runner = (COMPOSE, make_context_and_data, (RECURSE, (AXIS, 1), (AXIS, 2))).as_noun();
        
        expect_eval((44, runner.clone()), &b"too big"[..]);
        expect_eval((6, runner.clone()), &b"too small"[..]);
        expect_eval((42, runner.clone()), &b"correct"[..]);
    }
}
