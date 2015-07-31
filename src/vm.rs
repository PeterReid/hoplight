use std::{u32, usize};
use std::iter;

/// The maximum number of words a program may use.
const MEMORY_MAX: usize = 1024*1024;

pub struct Vm {
    memory: Vec<u32>,
    pc: u32,
    compute_cost: u64,
}

pub enum Fault {
    InvalidInstruction(u32),
    MemoryOutOfRange,
    ProgramTooLarge,
}

pub enum Request {
    Store,
}

mod opcode {
    pub const ADD: u32 = 1;
    
}

pub type OpResult = Result<Option<Request>, Fault>;

impl Vm {
    pub fn new(initial_memory: &[u32]) -> Result<Vm, Fault> {
        if initial_memory.len() > MEMORY_MAX {
            return Err(Fault::ProgramTooLarge);
        }
        
        let mut memory = Vec::with_capacity(MEMORY_MAX);
        memory.extend( initial_memory.iter().map(|x| *x).chain( iter::repeat(0u32) ).take(MEMORY_MAX) );
        
        Ok(Vm{
            memory: memory,
            pc: 0,
            compute_cost: 0,
        })
    }

    pub fn read_memory(&self, address: u32) -> u32 {
        assert!(usize::MAX >= MEMORY_MAX);
        
        if (usize::MAX as u64) < (u32::MAX as u64) {
            // It is possible for "address as usize" to overflow, so ensure'
            // we are small enough.
            if address >= MEMORY_MAX as u32 {
                return 0;
            }
        }
        
        self.memory.get(address as usize).map(|val| *val).unwrap_or(0)
    }
    
    fn read_pc_memory(&self, offset: u32) -> u32 {
        self.read_memory(self.pc.wrapping_add(offset))
    }
    
    fn write_memory(&mut self, address: u32, value: u32) -> Result<(), Fault> {
        assert!(usize::MAX >= MEMORY_MAX);
        
        if address < MEMORY_MAX as u32 {
            self.memory[address as usize] = value;
            Ok( () )
        } else {
            Err(Fault::MemoryOutOfRange)
        }
    }

    #[inline]
    fn advance_pc(&mut self, amount: u32) {
        self.pc = self.pc.wrapping_add(amount);
    }
    
    #[inline]
    fn incur_cost(&mut self, cost: u64) {
        self.compute_cost = self.compute_cost.saturating_add(cost);
    }
    
    fn exec_add(&mut self) -> OpResult {
        let src_address_1 = self.read_pc_memory(1);
        let src_address_2 = self.read_pc_memory(2);
        let dest = self.read_pc_memory(3);
        
        let addend_1 = self.read_memory(src_address_1);
        let addend_2 = self.read_memory(src_address_2);
        try!(self.write_memory(dest, addend_1.wrapping_add(addend_2)));
        
        self.advance_pc(4);
        self.incur_cost(1);
        self.compute_cost += 1;
        
        Ok(None)
    }
    
    pub fn step(&mut self) -> OpResult {
        let opcode = self.read_pc_memory(0);
        match opcode {
            opcode::ADD => self.exec_add(),
            
            unknown_opcode => Err(Fault::InvalidInstruction(unknown_opcode))
        }
    }
}

#[cfg(test)]
mod test {
    use super::{Vm, opcode};
    
    #[test]
    fn add() {
        let mut vm = Vm::new(&[
            opcode::ADD, 4,5,6,
            0x80000004, 0x90000003]).ok().unwrap();
        assert!(vm.step().ok().unwrap().is_none());
        assert_eq!(vm.read_memory(6), 0x10000007);
    }
}
