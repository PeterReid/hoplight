
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct CostError;

pub type CostResult<T> = Result<T, CostError>;


pub struct Ticks {
    count: u64
}

impl Ticks {
    pub fn new(tick_limit: u64) -> Ticks {
        Ticks{ count: tick_limit }
    }

    pub fn incur(&mut self, count: u64) -> CostResult<()> {
        self.count = try!(self.count.checked_sub(count).ok_or(CostError{}));
        Ok( () )
    }
}