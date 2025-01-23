#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct CostError;

pub type CostResult<T> = Result<T, CostError>;

#[derive(Clone)] // but not Copy, so we don't accidentally copy
pub struct Ticks {
    count: u64,
    initial: u64,
}

impl Ticks {
    pub fn new(tick_limit: u64) -> Ticks {
        Ticks { count: tick_limit, initial: tick_limit }
    }

    pub fn incur(&mut self, count: u64) -> CostResult<()> {
        self.count = self.count.checked_sub(count).ok_or(CostError {})?;
        Ok(())
    }

    pub fn get_consumed(&self) -> u64 {
        self.initial - self.count
    }
}
