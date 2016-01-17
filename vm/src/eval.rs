use noun::{Noun};
use axis::Axis;
use math;

#[derive(Debug, Eq, PartialEq)]
pub enum EvalError {
    Something,
    CellAsIndex,
    IndexOutOfRange,
    BadOpcode(u8),
    BadRecurseArgument,
    BadEqualsArgument,
    BadArgument,
    BadIfCondition,
    TickLimitExceeded,
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

struct Computation {
    ticks_used: u64,
    tick_cap: u64,
}

impl Computation {
    pub fn eval_on(&mut self, subject: &Noun, opcode: u8, argument: Noun) -> EvalResult {
        self.ticks_used += 1;
        if self.ticks_used >= self.tick_cap {
            return Err(EvalError::TickLimitExceeded);
        }
        match opcode {
            0 => subject.axis(&argument),
            1 => Ok(argument),
            2 => {
                if let Some((b, c)) = argument.into_cell() {
                    let b_result = try!(self.eval_pair(subject, b));
                    let c_result = try!(self.eval_pair(subject, c));
                    self.eval_pair(&b_result, c_result)
                } else {
                    Err(EvalError::BadRecurseArgument)
                }
            }
            3 => { // cell test
                Ok(Noun::from_bool(try!(self.eval_pair(subject, argument)).is_cell()))
            }
            4 => { // increment
                math::natural_add(&try!(self.eval_pair(subject, argument)), &Noun::from_u8(1))
            }
            5 => {
                if let Some((lhs, rhs)) = try!(self.eval_pair(subject, argument)).as_cell() {
                    Ok(lhs.equal(rhs))
                } else {
                    Err(EvalError::BadEqualsArgument)
                }
            }
            6 => {
                if let Some((b, c, d)) = into_triple(argument) {
                    let condition = try!(self.eval_pair(subject, b));
                    match condition.as_u8() {
                        Some(0) => {
                            self.eval_pair(subject, c)
                        }
                        Some(1) => {
                            self.eval_pair(subject, d)
                        }
                        _ => {
                            Err(EvalError::BadIfCondition)
                        }
                    }
                } else {
                    Err(EvalError::BadArgument)
                }
            }
            7 => {
                if let Some((b, c)) = argument.into_cell() {
                    let b_of_x = try!(self.eval_pair(subject, b));
                    self.eval_pair(&b_of_x, c)
                } else {
                    Err(EvalError::BadArgument)
                }
            }
            8 => {
                if let Some((b, c)) = argument.into_cell() {
                    let subject_prime = try!(self.eval_pair(subject, b));
                    self.eval_pair(&Noun::new_cell(subject_prime, subject.clone()), c)
                } else {
                    Err(EvalError::BadArgument)
                }
            }
            9 => {
                if let Some((b, c)) = argument.into_cell() {
                    let core = try!(self.eval_pair(subject, c));
                    let formula = try!(core.axis(&b));
                    self.eval_pair(&core, formula)
                } else {
                    Err(EvalError::BadArgument)
                }
            }
            _ => Err(EvalError::BadOpcode(opcode)),
        }
    }

    pub fn eval_pair(&mut self, subject: &Noun, formula: Noun) -> EvalResult {
        if let Some((operator, argument)) = formula.into_cell() {
            if let Some(opcode) = operator.as_byte() {
                return self.eval_on(subject, opcode, argument);
            } else if operator.is_cell() { // distribute
                return Ok(Noun::new_cell(
                    try!(self.eval_pair(subject, operator)),
                    try!(self.eval_pair(subject, argument)),
                ));
            }
        }
        Err(EvalError::Something)
    }
}


pub fn eval(expression: Noun, tick_limit: u64) -> EvalResult {;
    if let Some((subject, formula)) = expression.into_cell() {
        Computation{
            tick_cap: tick_limit,
            ticks_used: 0,
        }.eval_pair(&subject, formula)
    } else {
        Err(EvalError::Something)
    }
}

#[cfg(test)]
mod test {
    use noun::Noun;
    use as_noun::AsNoun;
    use eval::eval;

    fn expect_eval<E: AsNoun, R: AsNoun>(expression: E, result: R) {
        assert_eq!(
            eval(expression.as_noun(), 1000000),
            Ok( result.as_noun() )
        );
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
}
