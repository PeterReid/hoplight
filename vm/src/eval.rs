use axis::Axis;
use crypto::aead::{AeadDecryptor, AeadEncryptor};
use crypto::blake2b::Blake2b;
use crypto::chacha20poly1305::ChaCha20Poly1305;
use crypto::digest::Digest;
use deserialize::deserialize;
use equal::equal;
use noun::{Noun, NounKind};
use opcode::*;
use std::cmp::max;
use serialize::{self, SerializationError};
use shape::{reshape, length};
use std::convert::From;
use ticks::{CostError, Ticks};
use math::{add, invert, less, xor};

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
    DecryptionFailed,
    NonAtomicMath,
}

fn double_arg(noun: Noun) -> Result<(Noun, Noun), EvalError> {
    noun.into_cell().ok_or(EvalError::BadArgument)
}
fn triple_arg(noun: Noun) -> Result<(Noun, Noun, Noun), EvalError> {
    if let Some((a, bc)) = noun.into_cell() {
        if let Some((b, c)) = bc.into_cell() {
            return Ok((a, b, c));
        }
    }
    Err(EvalError::BadArgument)
}
fn bytes_arg(noun: &Noun) -> Result<&[u8], EvalError> {
    if let NounKind::Atom(xs) = noun.as_kind() {
        Ok(xs)
    } else {
        Err(EvalError::BadArgument)
    }
}
fn key_arg(noun: &Noun) -> Result<[u8; 32], EvalError> {
    let bytes = bytes_arg(noun)?;
    let mut key = [0u8; 32];
    if bytes.len() == 32 {
        key.copy_from_slice(bytes);
        Ok(key)
    } else {
        Err(EvalError::BadArgument)
    }
}

pub type EvalResult = Result<Noun, EvalError>;

pub trait SideEffectEngine {
    fn nearest_neighbor(&mut self, near: &[u8; 32]) -> [u8; 32];
    fn random(&mut self, _: &mut [u8]);
    fn load(&mut self, key: &[u8]) -> Option<Vec<u8>>;
    fn store(&mut self, key: &[u8], value: &[u8]);
    fn send(&mut self, destination: &[u8; 32], message: &[u8], local_cost: u64);
    fn secret(&self) -> &[u8; 32];
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

const SYMMETRIC_NONCE_LEN: usize = 8;
const SYMMETRIC_TAG_LEN: usize = 16;

impl<'a, S: SideEffectEngine> Computation<'a, S> {
    pub fn retrieve_with_tag(
        &mut self,
        subject: Noun,
        mut key: Vec<u8>,
        tag: u8,
    ) -> Result<Noun, EvalError> {
        key.push(tag);

        // TODO: It might be better to always return a cell.
        if let Some(xs) = self.side_effector.load(&key[..]) {
            let retrieved = deserialize(&xs[..]).map_err(|_| EvalError::StorageCorrupt)?;
            Ok(Noun::new_cell(
                Noun::from_bool(true),
                self.eval_on(subject, retrieved)?,
            ))
        } else {
            Ok(Noun::from_bool(false))
        }
    }

    /// TODO: This description is not very precise:
    /// The private key corresponding to an atom is only computable by the secret holder.
    /// the private key corresponding to a cell is computable by anyone, given knownledge
    /// of the private key of its right child.
    fn private_symmetric_key_for(
        &mut self,
        public: &Noun,
        branched_right: bool,
    ) -> Result<[u8; 32], EvalError> {
        match public.as_kind() {
            NounKind::Atom(xs) => {
                self.ticks_remaining.incur(xs.len() as u64)?;
                let mut result = [0u8; 32];

                Blake2b::blake2b(
                    &mut result[..],
                    &xs,
                    if branched_right {
                        &self.side_effector.secret()[..]
                    } else {
                        &[][..]
                    },
                );
                Ok(result)
            }
            NounKind::Cell(left, right) => {
                self.ticks_remaining.incur(128)?;
                let left_hash = self.private_symmetric_key_for(left, false)?;
                let right_hash = self.private_symmetric_key_for(right, true)?;
                let mut hasher = Blake2b::new(32);
                hasher.input(&left_hash[..]);
                hasher.input(&right_hash[..]);
                let mut output = [0u8; 32];
                hasher.result(&mut output[..]);
                Ok(output)
            }
        }
    }

    pub fn decrypt(
        &mut self,
        key: &[u8; 32],
        ciphertext: &[u8],
    ) -> Result<Option<Noun>, EvalError> {
        if ciphertext.len() < SYMMETRIC_NONCE_LEN + SYMMETRIC_TAG_LEN {
            return Ok(None);
        }
        let nonce = &ciphertext[0..SYMMETRIC_NONCE_LEN];
        let tag = &ciphertext[SYMMETRIC_NONCE_LEN..SYMMETRIC_NONCE_LEN + SYMMETRIC_TAG_LEN];
        let decryption_ciphertext = &ciphertext[SYMMETRIC_NONCE_LEN + SYMMETRIC_TAG_LEN..];

        self.ticks_remaining.incur(ciphertext.len() as u64)?;

        let mut decryptor = ChaCha20Poly1305::new(&key[..], &nonce[..], &[][..]);
        let mut plaintext_buffer = vec![0u8; decryption_ciphertext.len()];
        if !decryptor.decrypt(decryption_ciphertext, &mut plaintext_buffer[..], &tag[..]) {
            return Ok(None);
        }

        Ok(match deserialize(&plaintext_buffer[..]) {
            Err(_) => None,
            Ok(result) => Some(result),
        })
    }

    pub fn encrypt(&mut self, key: &[u8; 32], plaintext: &Noun) -> EvalResult {
        let result_buffer = self.serialize(&plaintext)?;

        let mut serialized =
            vec![0u8; SYMMETRIC_NONCE_LEN + SYMMETRIC_TAG_LEN + result_buffer.len()];
        {
            let (nonce, rest) = serialized.split_at_mut(SYMMETRIC_NONCE_LEN);
            let (tag, ciphertext) = rest.split_at_mut(SYMMETRIC_TAG_LEN);

            self.side_effector.random(&mut nonce[..]);

            let mut encryptor = ChaCha20Poly1305::new(&key[..], &nonce[..], &[][..]);

            encryptor.encrypt(&result_buffer[..], ciphertext, tag);
        }

        Ok(Noun::from_vec(serialized))
    }

    pub fn eval_on(&mut self, mut subject: Noun, mut formula: Noun) -> EvalResult {
        'tail_recurse: loop {
            self.ticks_remaining.incur(1)?;

            let (opcode_noun, argument) = formula.into_cell().ok_or(EvalError::AtomicFormula)?;
            if opcode_noun.is_cell() {
                // Distribute. The opcode and argument are actually both formulas.
                let lhs = self.eval_on(subject.clone(), opcode_noun)?;
                let rhs = self.eval_on(subject, argument)?;
                return Ok(Noun::new_cell(lhs, rhs));
            }

            let opcode = opcode_noun.as_u8().ok_or(EvalError::NotAnOpcode)?;

            return match opcode {
                AXIS => subject.axis(&argument),
                LITERAL => Ok(argument),
                RECURSE => {
                    if let Some((b, c)) = argument.into_cell() {
                        let b_result = self.eval_on(subject.clone(), b)?;
                        let c_result = self.eval_on(subject, c)?;
                        subject = b_result;
                        formula = c_result;
                        continue 'tail_recurse;
                    } else {
                        Err(EvalError::BadRecurseArgument)
                    }
                }
                IS_CELL => {
                    // cell test
                    Ok(Noun::from_bool(self.eval_on(subject, argument)?.is_cell()))
                }
                IS_EQUAL => {
                    if let Some((lhs, rhs)) = self.eval_on(subject, argument)?.as_cell() {
                        Ok(Noun::from_bool(equal(lhs, rhs, &mut self.ticks_remaining)?))
                    } else {
                        Err(EvalError::BadEqualsArgument)
                    }
                }
                IF => {
                    let (b, c, d) = triple_arg(argument)?;
                    let condition = self.eval_on(subject.clone(), b)?;
                    match condition.as_u8() {
                        Some(0) => {
                            formula = c;
                            continue 'tail_recurse;
                        }
                        Some(1) => {
                            formula = d;
                            continue 'tail_recurse;
                        }
                        _ => Err(EvalError::BadIfCondition),
                    }
                }
                COMPOSE => {
                    if let Some((b, c)) = argument.into_cell() {
                        let b_of_x = self.eval_on(subject, b)?;
                        subject = b_of_x;
                        formula = c;
                        continue 'tail_recurse;
                    } else {
                        Err(EvalError::BadArgument)
                    }
                }
                DEFINE => {
                    if let Some((b, c)) = argument.into_cell() {
                        let subject_prime = self.eval_on(subject.clone(), b)?;
                        subject = Noun::new_cell(subject_prime, subject);
                        formula = c;
                        continue 'tail_recurse;
                    } else {
                        Err(EvalError::BadArgument)
                    }
                }
                CALL => {
                    if let Some((b, c)) = argument.into_cell() {
                        let core = self.eval_on(subject, c)?;
                        let inner_formula = core.axis(&b)?;
                        subject = core;
                        formula = inner_formula;
                        continue 'tail_recurse;
                    } else {
                        Err(EvalError::BadArgument)
                    }
                }
                HASH => {
                    // hash
                    let hash_target = self.eval_on(subject, argument)?;
                    let buffer = self.serialize(&hash_target)?;
                    self.ticks_remaining.incur(20 + (buffer.len() as u64))?;
                    let mut result = [0u8; 64];
                    Blake2b::blake2b(&mut result[..], &buffer, &[][..]);
                    Ok(Noun::from_slice(&result[..]))
                }
                STORE_BY_HASH => {
                    // store by hash
                    let hash_target = self.eval_on(subject, argument)?;
                    let buffer = self.serialize(&hash_target)?;
                    self.ticks_remaining.incur(20 + (buffer.len() as u64))?;
                    let mut result = [0u8; 64 + 1];
                    result[64] = 1;
                    Blake2b::blake2b(&mut result[..64], &buffer, &[][..]);
                    self.side_effector.store(&result[..], &buffer[..]);
                    Ok(Noun::from_bool(true)) // TODO: It might be better to return the hash
                }
                RETRIEVE_BY_HASH => {
                    // retrieve by hash
                    let hash = self.eval_on(subject.clone(), argument)?;
                    if let Some(hash_bytes) = hash.into_vec() {
                        self.retrieve_with_tag(subject, hash_bytes, 1)
                    } else {
                        Ok(Noun::from_bool(false))
                    }
                }
                STORE_BY_KEY => {
                    if let Some((key, value)) = self.eval_on(subject, argument)?.into_cell() {
                        let mut storage_key = self.serialize(&key)?;
                        storage_key.push(0);
                        let storage_value = self.serialize(&value)?;
                        self.side_effector
                            .store(&storage_key[..], &storage_value[..]);
                        Ok(Noun::from_bool(true))
                    } else {
                        Err(EvalError::BadArgument)
                    }
                }
                RETRIEVE_BY_KEY => {
                    let key = self.eval_on(subject.clone(), argument)?;
                    let key_bytes = self.serialize(&key)?;
                    self.retrieve_with_tag(subject, key_bytes, 0)
                }
                RANDOM => {
                    let length = self
                        .eval_on(subject, argument)?
                        .as_usize()
                        .ok_or(EvalError::InvalidLength)?;
                    if length > 1_000_000 {
                        return Err(EvalError::InvalidLength);
                    }
                    let mut xs = vec![0u8; length];
                    self.side_effector.random(&mut xs);
                    Ok(Noun::from_vec(xs))
                }
                RESHAPE => {
                    if let Some((data, structure)) = self.eval_on(subject, argument)?.into_cell() {
                        reshape(&data, &structure, &mut self.ticks_remaining, 10_000_000)
                            .map_err(|_| EvalError::BadShape)
                    } else {
                        Err(EvalError::BadArgument)
                    }
                }
                SHAPE => {
                    let data = self.eval_on(subject, argument)?;
                    Ok(length(&data, &mut self.ticks_remaining)?)
                }
                ADD => {
                    if let Some((lhs, rhs)) = self.eval_on(subject, argument)?.into_cell() {
                        self.ticks_remaining.incur(max(lhs.atom_len().unwrap_or(0), rhs.atom_len().unwrap_or(0)) as u64)?;
                        add(&lhs, &rhs).ok_or(EvalError::NonAtomicMath)
                    } else {
                        Err(EvalError::BadArgument)
                    }
                }
                LESS => {
                    if let Some((lhs, rhs)) = self.eval_on(subject, argument)?.into_cell() {
                        self.ticks_remaining.incur(max(lhs.atom_len().unwrap_or(0), rhs.atom_len().unwrap_or(0)) as u64)?;
                        less(&lhs, &rhs).map(Noun::from_bool).ok_or(EvalError::NonAtomicMath)
                    } else {
                        Err(EvalError::BadArgument)
                    }
                }
                XOR => {
                    if let Some((lhs, rhs)) = self.eval_on(subject, argument)?.into_cell() {
                        self.ticks_remaining.incur(max(lhs.atom_len().unwrap_or(0), rhs.atom_len().unwrap_or(0)) as u64)?;
                        xor(&lhs, &rhs).ok_or(EvalError::NonAtomicMath)
                    } else {
                        Err(EvalError::BadArgument)
                    }
                }
                INVERT => {
                    let data = self.eval_on(subject, argument)?;
                    self.ticks_remaining.incur(data.atom_len().unwrap_or(0) as u64)?;
                    invert(&data).ok_or(EvalError::NonAtomicMath)
                }
                GENERATE_KEYPAIR => {
                    let provided_seed = self.eval_on(subject, argument)?;
                    let mut random_seed = vec![0u8; 32];
                    self.side_effector.random(&mut random_seed[..]);
                    let public = Noun::new_cell(provided_seed, Noun::from_vec(random_seed));
                    let private =
                        Noun::from_slice(&self.private_symmetric_key_for(&public, false)?[..]);
                    Ok(Noun::new_cell(private, public))
                }
                DECRYPT => {
                    let (private_key, ciphertext) = double_arg(self.eval_on(subject, argument)?)?;

                    if let Some(plaintext) =
                        self.decrypt(&key_arg(&private_key)?, bytes_arg(&ciphertext)?)?
                    {
                        Ok(Noun::new_cell(Noun::from_bool(true), plaintext))
                    } else {
                        Ok(Noun::from_bool(false))
                    }
                }
                ENCRYPT => {
                    let (private_key, plaintext) = double_arg(self.eval_on(subject, argument)?)?;
                    self.encrypt(&key_arg(&private_key)?, &plaintext)
                }
                EXUCRYPT => {
                    let (public_key, request_ciphertext) =
                        double_arg(self.eval_on(subject.clone(), argument)?)?;
                    let private_key = self.private_symmetric_key_for(&public_key, false)?;

                    // Decryption
                    let program = if let Some(program) =
                        self.decrypt(&private_key, bytes_arg(&request_ciphertext)?)?
                    {
                        program
                    } else {
                        return Ok(Noun::from_bool(false));
                    };

                    // Evaluation
                    let result = self.eval_on(subject, program)?;

                    Ok(Noun::new_cell(
                        Noun::from_bool(true),
                        self.encrypt(&private_key, &result)?,
                    ))
                }
                //11 => { // send
                //    if let Some((b, c, d)) =
                //}
                _ => Err(EvalError::BadOpcode(opcode)),
            };
        }
    }

    fn serialize(&mut self, noun: &Noun) -> Result<Vec<u8>, EvalError> {
        match serialize::serialize(noun, 1_000_000) {
            Ok(x) => Ok(x),
            Err(SerializationError::OverlongAtom) => Err(EvalError::BadArgument),
            Err(SerializationError::MaximumLengthExceeded) => Err(EvalError::MemoryExceeded),
        }
    }
}

pub fn eval<S: SideEffectEngine>(
    expression: Noun,
    side_effector: &mut S,
    tick_limit: u64,
) -> EvalResult {
    if let Some((subject, formula)) = expression.into_cell() {
        Computation {
            ticks_remaining: Ticks::new(tick_limit),
            side_effector: side_effector,
        }
        .eval_on(subject, formula)
    } else {
        Err(EvalError::EvalOnAtom)
    }
}

#[cfg(test)]
mod test {
    use as_noun::AsNoun;
    use chacha::{ChaCha, KeyStream};
    use crypto::blake2b::Blake2b;
    use eval::{eval, SideEffectEngine};
    use noun::Noun;
    use opcode::*;
    use serialize;
    use std::collections::HashMap;

    struct TestSideEffectEngine {
        storage: HashMap<Vec<u8>, Vec<u8>>,
        rng: ChaCha,
    }

    impl TestSideEffectEngine {
        fn new() -> TestSideEffectEngine {
            TestSideEffectEngine {
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
        fn send(&mut self, _destination: &[u8; 32], _message: &[u8], _local_cost: u64) {}
        fn secret(&self) -> &[u8; 32] {
            b"this is a thirty-two byte secret"
        }
    }

    fn eval_simple<E: AsNoun>(expression: E) -> Noun {
        let mut engine = TestSideEffectEngine::new();
        eval(expression.as_noun(), &mut engine, 1000000)
            .expect("eval_simple expression got an error")
    }

    fn expect_eval_with<E: AsNoun, R: AsNoun>(
        engine: &mut TestSideEffectEngine,
        expression: E,
        result: R,
    ) {
        assert_eq!(
            eval(expression.as_noun(), engine, 1000000),
            Ok(result.as_noun())
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

        expect_eval(((76, 30), 1, (42, 60)), (42, 60));
    }

    #[test]
    fn axis_op() {
        expect_eval((99, 0, 1), 99);
        expect_eval(((98, 99), 0, 2), 98);
        expect_eval(((98, 99), 0, 3), 99);

        expect_eval(
            (
                (1, 2, 3, 4, (5, 6, 7, (8, 9, 10, 11))),
                0,
                &[0xff, 0x07][..],
            ),
            11,
        );
        expect_eval(((((1, 2), 3), 4), 0, 5), 3);
        expect_eval(((((1, 2), 3), 4), 0, 4), (1, 2));
    }

    #[test]
    fn recurse_op() {
        expect_eval(((123, (0, 1)), 2, (0, 2), (0, 3)), 123);
    }

    #[test]
    fn equal_op() {
        expect_eval(((5, 5), 5, (0, 1)), Noun::from_bool(true));
        expect_eval(((5, 8), 5, (0, 1)), Noun::from_bool(false));
    }

    #[test]
    fn cell_op() {
        expect_eval(((99, 33), 3, (0, 1)), Noun::from_bool(true));
        expect_eval((99, 3, (0, 1)), Noun::from_bool(false));
    }

    #[test]
    fn increment_op() {
        let hash_target = (5, 3, &b"longer atom"[..]).as_noun();
        expect_eval((hash_target.clone(), HASH, (0, 1)), hash(hash_target));
    }
    
    

    #[test]
    fn distribute() {
        expect_eval(
            (22, (HASH, (0, 1)), (0, 1), (1, 50)),
            (hash(22), 22, 50).as_noun(),
        );
    }

    #[test]
    fn if_true() {
        expect_eval((42, (6, (1, 0), (HASH, 0, 1), (1, 233))), hash(42));
    }

    #[test]
    fn if_false() {
        expect_eval((42, (6, (1, 1), (4, 0, 1), (1, 233))), 233);
    }

    #[test]
    fn composition() {
        expect_eval((42, (7, (HASH, 0, 1), (HASH, 0, 1))), hash(hash(42)));
    }

    #[test]
    fn push_1() {
        expect_eval((42, (8, (HASH, 0, 1), (0, 1))), (hash(42), 42));
    }

    #[test]
    fn push_2() {
        expect_eval((42, (8, (HASH, 0, 1), (HASH, 0, 3))), hash(42));
    }

    #[test]
    fn decrement() {
        expect_eval(
            (
                iterate_hash(42),
                (
                    8,
                    (1, 0),
                    8,
                    (
                        1,
                        6,
                        (5, (0, 7), HASH, 0, 6),
                        (0, 6),
                        (9, 2, (0, 2), (HASH, 0, 6), 0, 7),
                    ),
                    (9, 2, 0, 1),
                ),
            ),
            iterate_hash(41),
        );
    }

    #[test]
    fn store_and_get_hash() {
        let mut engine = expect_eval(
            (
                21,
                RECURSE,
                (
                    (STORE_BY_HASH, ((LITERAL, LITERAL), (AXIS, 1))),
                    ((AXIS, 1), (LITERAL, 2), (AXIS, 1)),
                ),
                (LITERAL, AXIS, 3),
            ),
            (21, 2, 21),
        );
        let hash = eval_simple((21, HASH, ((LITERAL, LITERAL), (AXIS, 1))));
        expect_eval_with(&mut engine, (hash, (RETRIEVE_BY_HASH, (AXIS, 1))), (0, 21));
    }

    #[test]
    fn store_and_get_key() {
        let mut engine = expect_eval(
            (
                &b"orange"[..],
                (
                    STORE_BY_KEY,
                    (LITERAL, &b"color"[..]),
                    ((LITERAL, LITERAL), (AXIS, 1)),
                ),
            ),
            Noun::from_bool(true),
        );
        expect_eval_with(
            &mut engine,
            (&b"color"[..], (RETRIEVE_BY_KEY, (AXIS, 1))),
            (0, &b"orange"[..]),
        );
    }

    #[test]
    fn gen_random() {
        let random = eval_simple((20, RANDOM, (AXIS, 1)));
        let buf = random.into_vec().expect("random should have made an atom");
        assert_eq!(buf.len(), 20);
        // These bytes will probably be different.
        // We use a deterministic RNG, so a this failure won't be intermittent.
        // I do want to catch leaving this uninitialized somehow.
        assert!(buf[0] != buf[1] || buf[1] != buf[2]);
    }

    fn hash<T: AsNoun>(x: T) -> Noun {
        let buffer = serialize::serialize(&x.as_noun(), 100000).expect("hash serialization failed");
        let mut result = [0u8; 64];
        Blake2b::blake2b(&mut result[..], &buffer, &[][..]);
        Noun::from_slice(&result[..])
    }

    fn iterate_hash(rounds: usize) -> Noun {
        let mut x = Noun::from_u8(0);
        for _ in 0..rounds {
            x = hash(x);
        }
        x
    }

    #[test]
    fn guessing_game() {
        let rightleftleft = 12;
        let rightleftright = 13;
        let rightright = 7;
        let left = 2;
        let rightleft = 6;

        let f = (
            IF,
            (IS_EQUAL, (AXIS, rightleftleft), (AXIS, rightleftright)),
            (LITERAL, &b"correct"[..]),
            (
                IF,
                (IS_EQUAL, (AXIS, rightleftleft), (AXIS, rightright)),
                (LITERAL, &b"too small"[..]),
                (
                    IF,
                    (IS_EQUAL, (AXIS, rightleftright), (AXIS, rightright)),
                    (LITERAL, &b"too big"[..]),
                    (
                        RECURSE,
                        (
                            (AXIS, left),
                            ((AXIS, rightleft), (HASH, (AXIS, rightright))),
                        ),
                        (AXIS, left),
                    ),
                ),
            ),
        )
            .as_noun();
        let make_context_and_data = (
            (LITERAL, f.clone()),
            (((AXIS, 1), (LITERAL, iterate_hash(42))), (LITERAL, 0)),
        )
            .as_noun();

        let runner = (
            COMPOSE,
            make_context_and_data,
            (RECURSE, (AXIS, 1), (AXIS, 2)),
        )
            .as_noun();

        expect_eval((iterate_hash(44), runner.clone()), &b"too big"[..]);
        expect_eval((iterate_hash(6), runner.clone()), &b"too small"[..]);
        expect_eval((iterate_hash(42), runner.clone()), &b"correct"[..]);
    }

    #[test]
    fn encrypt_decrypt() {
        let key: Vec<u8> = (4..36).collect();
        assert!(key.len() == 32);
        expect_eval(
            (key, DECRYPT, (AXIS, 1), (ENCRYPT, (AXIS, 1), (LITERAL, 21))),
            (true, 21),
        );
    }

    #[test]
    fn encrypt_decrypt_key_mismatch() {
        let key_one: Vec<u8> = (4..36).collect();
        let key_two: Vec<u8> = (104..136).collect();
        assert!(key_one.len() == 32);
        assert!(key_two.len() == 32);
        expect_eval(
            (
                key_one,
                DECRYPT,
                (LITERAL, key_two),
                (ENCRYPT, (AXIS, 1), (LITERAL, 21)),
            ),
            false,
        );
    }
}
