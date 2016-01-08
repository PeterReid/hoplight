use noun::Noun;
use std::rc::Rc;

pub trait AsNoun {
    fn as_noun(self) -> Noun;
}

impl AsNoun for Noun {
    fn as_noun(self) -> Noun {
        self
    }
}

impl AsNoun for u8 {
    fn as_noun(self) -> Noun {
        Noun::from_u8(self)
    }
}

impl<'a> AsNoun for &'a [u8] {
    fn as_noun(self) -> Noun {
        Noun::Atom(Rc::new(self.to_vec()))
    }
}

impl<A: AsNoun, B:AsNoun> AsNoun for (A, B) {
    fn as_noun(self) -> Noun {
        Noun::new_cell(self.0.as_noun(), self.1.as_noun())
    }
}

impl<A: AsNoun, B:AsNoun, C:AsNoun> AsNoun for (A, B, C) {
    fn as_noun(self) -> Noun {
        Noun::new_cell(self.0.as_noun(), (self.1, self.2).as_noun())
    }
}

impl<A: AsNoun, B:AsNoun, C:AsNoun, D:AsNoun> AsNoun for (A, B, C, D) {
    fn as_noun(self) -> Noun {
        Noun::new_cell(self.0.as_noun(), (self.1, self.2, self.3).as_noun())
    }
}
impl<A: AsNoun, B:AsNoun, C:AsNoun, D:AsNoun, E:AsNoun> AsNoun for (A, B, C, D, E) {
    fn as_noun(self) -> Noun {
        Noun::new_cell(self.0.as_noun(), (self.1, self.2, self.3, self.4).as_noun())
    }
}
impl<A: AsNoun, B:AsNoun, C:AsNoun, D:AsNoun, E:AsNoun, F:AsNoun> AsNoun for (A, B, C, D, E, F) {
    fn as_noun(self) -> Noun {
        Noun::new_cell(self.0.as_noun(), (self.1, self.2, self.3, self.4, self.5).as_noun())
    }
}
impl<A: AsNoun, B:AsNoun, C:AsNoun, D:AsNoun, E:AsNoun, F:AsNoun, G:AsNoun> AsNoun for (A, B, C, D, E, F, G) {
    fn as_noun(self) -> Noun {
        Noun::new_cell(self.0.as_noun(), (self.1, self.2, self.3, self.4, self.5, self.6).as_noun())
    }
}

#[test]
fn as_nouning() {
    assert_eq!((3, 6, 9, 12, (15, 16), 18).as_noun(), (3, (6, 9, (12, ((15, 16), 18)))).as_noun());
}
