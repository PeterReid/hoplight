use noun::Noun;
use ticks::{CostResult, Ticks};

/// Compare two `Noun`s for value equality. That is, they would have identical serializations.
/// Since a noun could be of unbounded size, this computation is limited with a tick count.
pub fn equal(a: &Noun, b: &Noun, ticks: &mut Ticks) -> CostResult<bool> {
    try!(ticks.incur(1));
    Ok(match (a, b) {
        (&Noun::Cell(ref a, ref b), &Noun::Cell(ref x, ref y)) => equal(a, x, ticks)? && equal(b, y, ticks)?,
        (&Noun::SmallAtom{value:value_a, length:length_a}, &Noun::SmallAtom{value:value_b, length:length_b}) => (value_a,length_a)==(value_b,length_b),
        (&Noun::Atom(ref a), &Noun::Atom(ref x)) => a==x,
        _ => false // Nouns that can be SmallAtoms will be SmallAtoms. Doing otherwise would complicate constant-time guarantees.
    })
}

#[cfg(test)]
mod test {
    use as_noun::AsNoun;
    use noun::Noun;
    use ticks::Ticks;
    use equal::equal;

    #[test]
    fn giant_equality() {
        let mut a = Noun::from_u8(0);

        // Double a 40 times, leading to a terabyte-scale serialization
        for _ in 0..40 {
            a = Noun::new_cell(a.clone(), a.clone());
        }

        assert!(equal(&a, &a, &mut Ticks::new(1000)).is_err());
    }


    #[test]
    fn are_equal() {
        assert_eq!(equal(
            &(6, 7, &b"element three"[..]).as_noun(),
            &(6, (7, &b"element three"[..])).as_noun(),
            &mut Ticks::new(1000)), Ok(true));
    }

    #[test]
    fn not_equal() {
        assert_eq!(equal(
            &(6, 7, &b"element three"[..]).as_noun(),
            &(6, (9, &b"element three"[..])).as_noun(),
            &mut Ticks::new(1000)), Ok(false));
    }
}
