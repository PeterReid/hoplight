use noun::Noun;
use std::ops::Deref;

#[derive(Debug, Eq, PartialEq)]
pub enum EvalError {
    Something,
    CellAsIndex,
    IndexOutOfRange,
    BadOpcode(u8),
}

pub type EvalResult = Result<Noun, EvalError>;

pub fn eval_on(subject: &Noun, opcode: u8, argument: &Noun) -> EvalResult {
    println!("Evaluating opcode {}", opcode);
    match opcode {
        0 => subject.axis(argument),
        1 => Ok(argument.clone()),
        _ => Err(EvalError::BadOpcode(opcode)),
    }
}

pub fn eval(expression: &Noun) -> EvalResult {
    if let &Noun::Cell(ref subject, ref formula) = expression {
        if let &Noun::Cell(ref operator, ref argument) = formula.deref() {
            if let Some(opcode) = operator.as_byte() {
                return eval_on(subject, opcode, argument);
            }
        }
    }
    Err(EvalError::Something)
}

#[cfg(test)]
mod test {
    use as_noun::AsNoun;
    use eval::eval;

    fn expect_eval<E: AsNoun, R: AsNoun>(expression: E, result: R) {
        assert_eq!(
            eval(&expression.as_noun()),
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
}
