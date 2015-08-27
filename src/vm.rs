use std::{u32, usize};
use std::cmp::Ordering;
use std::iter::{self};
use crypto::blake2b::Blake2b;
use crypto::digest::Digest;

/// The maximum number of words a program may use.
const MEMORY_MAX: usize = 1024 * 1024;
const WORD_READ_MAX: u32 = 1024 * 1024;
pub struct Vm {
    memory: Vec<u32>,
    pc: u32,
    compute_cost: u64,
}

pub enum Fault {
    InvalidInstruction(u32),
    MemoryOutOfRange,
    ProgramTooLarge,
    ReadTooLarge,
}

pub enum Request {
    StoreData{ key: Vec<u32>, value: Vec<u32> },
    LoadData{ key: Vec<u32>, destination_address: u32, destination_word_count: u32 },
    Send{ recipient: Vec<u32>, message: Vec<u32> },
    LoadSelfAddress{ destination_address: u32 },
    LoadNearestNeighbors{ near_to: Vec<u32>, count: u32, destination_address: u32 },
}

mod opcode {
    pub const ADD: u32 = 1;
    pub const STORE_HASH: u32 = 2;
    pub const LOAD_HASH: u32 = 3;
    pub const SEND: u32 = 4;
    pub const JUMP_IF_ZERO: u32 = 5;
    pub const VECTOR_COMPARE: u32 = 6;
    pub const LOAD_SELF_ADDRESS: u32 = 7;
    pub const LOAD_NEAREST_NEIGHBORS: u32 = 8;
}

pub type OpResult = Result<Option<Request>, Fault>;

fn words_to_le_bytes(words: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(words.len()*4);
    for word in words.iter() {
        bytes.push(((word>> 0) & 0xffu32) as u8);
        bytes.push(((word>> 8) & 0xffu32) as u8);
        bytes.push(((word>>16) & 0xffu32) as u8);
        bytes.push(((word>>24) & 0xffu32) as u8);
    }
    
    bytes
}

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
    
    /// Record CPU cost
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
        
        Ok(None)
    }
    
    /// Read contiguous words from memory.
    fn read_memory_words(&self, start_address: u32, word_count: u32) -> Result<Vec<u32>, Fault> {
        if word_count >= WORD_READ_MAX {
            // Avoid denial of service from a huge memory allocation.
            return Err(Fault::ReadTooLarge);
        }
        
        let words = (0..word_count).map(|offset| {
            self.read_memory(start_address.wrapping_add(offset))
        }).collect();
        
        return Ok(words);
    }
    
    fn exec_store_hash(&mut self) -> OpResult {
        let start_word = self.read_pc_memory(1);
        let word_count = self.read_pc_memory(2);
        
        let data = try!(self.read_memory_words(start_word, word_count));
        
        let mut hash = [0u8; 64];//Vec::from_iter( iter::repeat(0u8).take(64) );
        let mut hasher = Blake2b::new(hash.len());
        hasher.input(&words_to_le_bytes(&data));
        hasher.result(&mut hash[..]);
        
        self.advance_pc(3);
        self.incur_cost((word_count as u64) * 1000);
        
        let hash_words = (0..16).map(|word_offset| word_offset*4).map(|x| {
            ((hash[x+0] as u32)<< 0) | 
            ((hash[x+1] as u32)<< 8) |
            ((hash[x+2] as u32)<<16) |
            ((hash[x+3] as u32)<<24)
        }).collect();
        
        Ok(Some(Request::StoreData{ key: hash_words, value: data }))
    }
    
    fn exec_load_data(&mut self) -> OpResult {
        let hash_start_word = self.read_pc_memory(1);
        let dest_start_word = self.read_pc_memory(2);
        let dest_word_count = self.read_pc_memory(3);
        
        let hash = try!(self.read_memory_words(hash_start_word, 64/4));
        
        self.advance_pc(4);
        self.incur_cost(1);
        
        Ok(Some(Request::LoadData{ key: hash, destination_address: dest_start_word, destination_word_count: dest_word_count }))
    }
    
    fn exec_send(&mut self) -> OpResult {
        let recipient_start = self.read_pc_memory(1);
        let message_start = self.read_pc_memory(2);
        let message_word_count = self.read_pc_memory(3);
        
        let recipient = try!(self.read_memory_words(recipient_start, 8));
        let message = try!(self.read_memory_words(message_start, message_word_count));
        
        self.advance_pc(4);
        self.incur_cost(30 + (message_word_count as u64) * 1);
        
        Ok(Some(Request::Send{ recipient: recipient, message: message }))
    }
    
    fn exec_jump_if_zero(&mut self) -> OpResult {
        let jump_target = self.read_pc_memory(1);
        let condition_address = self.read_pc_memory(2);
        let condition = self.read_memory(condition_address);
        
        if condition != 0 {
            self.pc = jump_target;
        } else {
            self.advance_pc(3);
        }
        self.incur_cost(1);
        
        Ok(None)
    }
    
    fn exec_vector_compare(&mut self) -> OpResult {
        let src_1_start = self.read_pc_memory(1);
        let src_2_start = self.read_pc_memory(2);
        let word_length = self.read_pc_memory(3);
        let destination = self.read_pc_memory(4);
        self.advance_pc(5);
        
        let src_1 = try!(self.read_memory_words(src_1_start, word_length));
        let src_2 = try!(self.read_memory_words(src_2_start, word_length));
        
        let result = match src_1.cmp(&src_2) {
            Ordering::Less => 0xffffffff,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        };
        
        try!(self.write_memory(destination, result));
        
        Ok(None)
    }
    
    fn exec_load_self_address(&mut self) -> OpResult {
        let destination = self.read_pc_memory(1);
        self.advance_pc(2);
        
        Ok(Some(Request::LoadSelfAddress{
            destination_address: destination
        }))
    }
    
    fn exec_nearest_neighbors(&mut self) -> OpResult {
        let near_start = self.read_pc_memory(1);
        let destination_start = self.read_pc_memory(2);
        let count = self.read_pc_memory(3);
        self.advance_pc(4);
        
        let near_to = try!(self.read_memory_words(near_start, 8));
        
        Ok(Some(Request::LoadNearestNeighbors{
            near_to: near_to,
            count: count,
            destination_address: destination_start,
        }))
    }
    
    
    pub fn step(&mut self) -> OpResult {
        let opcode = self.read_pc_memory(0);
        match opcode {
            opcode::ADD => self.exec_add(),
            opcode::STORE_HASH => self.exec_store_hash(),
            opcode::LOAD_HASH => self.exec_load_data(),
            opcode::SEND => self.exec_send(),
            opcode::JUMP_IF_ZERO => self.exec_jump_if_zero(),
            opcode::VECTOR_COMPARE => self.exec_vector_compare(),
            opcode::LOAD_SELF_ADDRESS => self.exec_load_self_address(),
            opcode::LOAD_NEAREST_NEIGHBORS => self.exec_nearest_neighbors(),
            unknown_opcode => Err(Fault::InvalidInstruction(unknown_opcode))
        }
    }
}

#[cfg(test)]
mod test {
    use super::{Vm, opcode, Request};
    
    #[test]
    fn add() {
        let mut vm = Vm::new(&[
            opcode::ADD, 4,5,6,
            0x80000004, 0x90000003]).ok().unwrap();
        assert!(vm.step().ok().unwrap().is_none());
        assert_eq!(vm.read_memory(6), 0x10000007);
    }

    #[test]
    fn hash() {
        let mut vm = Vm::new(&[
            opcode::STORE_HASH, 3,2,
            0x04030201, 0x00000005]).ok().unwrap();
        match vm.step() {
            Ok(Some(Request::StoreData{key, value})) => {
                assert_eq!(value, [0x04030201, 0x00000005].to_vec());
                assert_eq!(key, [ // b2sum of the value
                    0x56d315bd, 0x5557459d, 0x690186f3, 0xe1e38838,
                    0x9eecbd81, 0xfe08f530, 0x1d3da984, 0xef68c7a8,
                    0x7e5558ff, 0x70c8610c, 0x160b5a73, 0x1edf1571,
                    0x93d126ba, 0x00a2a0ae, 0xd42fd6ac, 0x9aa6461c,
                ].to_vec());
            }
            _ => { panic!("Expected a store request") }
        }
    }
    
    #[test]
    fn send() {
        let mut vm = Vm::new(&[
            opcode::SEND, /* recipient start = */ 6, /*message start = */ 4, /*message words = */ 2,
            0x04030201, 0x08070605, 
            // recipient
            0x11111111, 0x22222222, 0x33333333, 0x44444444,
            0x55555555, 0x66666666, 0x77777777, 0x88888888
            ]).ok().unwrap();
        match vm.step() {
            Ok(Some(Request::Send{recipient, message})) => {
                assert_eq!(recipient, [
                    0x11111111, 0x22222222, 0x33333333, 0x44444444,
                    0x55555555, 0x66666666, 0x77777777, 0x88888888].to_vec());
                assert_eq!(message, [0x04030201, 0x08070605].to_vec());
            }
            _ => { panic!("Expected a store request") }
        }
    }
}
