use std::{u32, usize};
use std::iter::{self, FromIterator};
use crypto::blake2b::Blake2b;
use crypto::digest::Digest;

/// The maximum number of words a program may use.
const MEMORY_MAX: usize = 1024 * 1024;
const BYTE_READ_MAX: u32 = 1024 * 1024 * 4;
pub struct Vm {
    memory: Vec<u32>,
    pc: u32,
    compute_cost: u64,
}

pub enum Fault {
    InvalidInstruction(u32),
    MemoryOutOfRange,
    ProgramTooLarge,
    ByteReadTooLarge,
}

pub enum Request {
    StoreData{ key: Vec<u8>, value: Vec<u8> },
    LoadData{ key: Vec<u8>, destination_address: u32, destination_length: u32 },
    Send{ recipient: Vec<u8>, message: Vec<u8> },
}

mod opcode {
    pub const ADD: u32 = 1;
    pub const STORE_HASH: u32 = 2;
    pub const LOAD_HASH: u32 = 3;
    pub const SEND: u32 = 4;
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
    
    /// Read contiguous bytes from memory. The start_address refers
    /// to the initial word's index, so the read's start must be 
    /// word-aligned. 
    /// Words are interpreted as little-endian; the least-significant
    /// byte of each word comes first.
    fn read_memory_bytes(&self, start_address: u32, byte_length: u32) -> Result<Vec<u8>, Fault> {
        if byte_length >= BYTE_READ_MAX {
            // Avoid denial of service from a huge memory allocation.
            return Err(Fault::ByteReadTooLarge);
        }
        
        let mut bs = Vec::with_capacity(byte_length as usize);
        
        let mut byte_length_remaining = byte_length;
        let mut read_address = start_address;
        while byte_length_remaining >= 4 {
            let word = self.read_memory(read_address);
            bs.push(((word>> 0) & 0xff) as u8);
            bs.push(((word>> 8) & 0xff) as u8);
            bs.push(((word>>16) & 0xff) as u8);
            bs.push(((word>>24) & 0xff) as u8);
            byte_length_remaining -= 4;
            read_address = read_address.wrapping_add(1);
        }
        
        let mut final_word = self.read_memory(read_address);
        while byte_length_remaining > 0 {
            bs.push((final_word & 0xff) as u8);
            final_word = final_word >> 8;
            byte_length_remaining -= 1;
        }
        
        assert_eq!(bs.len() as u32, byte_length);
        
        return Ok(bs);
    }
    
    fn exec_store_hash(&mut self) -> OpResult {
        let start_word = self.read_pc_memory(1);
        let byte_length = self.read_pc_memory(2);
        
        let data = try!(self.read_memory_bytes(start_word, byte_length));
        
        let mut hash = Vec::from_iter( iter::repeat(0u8).take(64) );
        let mut hasher = Blake2b::new(hash.len());
        hasher.input(&data);
        hasher.result(&mut hash[..]);
        
        self.advance_pc(3);
        self.incur_cost((byte_length as u64) * 1000);
        
        Ok(Some(Request::StoreData{ key: hash, value: data }))
    }
    
    fn exec_load_data(&mut self) -> OpResult {
        let hash_start_word = self.read_pc_memory(1);
        let dest_start_word = self.read_pc_memory(2);
        let dest_byte_length = self.read_pc_memory(3);
        
        let hash = try!(self.read_memory_bytes(hash_start_word, 64));
        
        self.advance_pc(4);
        self.incur_cost(1);
        
        Ok(Some(Request::LoadData{ key: hash, destination_address: dest_start_word, destination_length: dest_byte_length }))
    }
    
    fn exec_send(&mut self) -> OpResult {
        let recipient_start = self.read_pc_memory(1);
        let message_start = self.read_pc_memory(2);
        let message_byte_length = self.read_pc_memory(3);
        
        let recipient = try!(self.read_memory_bytes(recipient_start, 32));
        let message = try!(self.read_memory_bytes(message_start, message_byte_length));
        
        self.advance_pc(4);
        self.incur_cost(30 + (message_byte_length as u64) * 1);
        
        Ok(Some(Request::Send{ recipient: recipient, message: message }))
    }
    
    pub fn step(&mut self) -> OpResult {
        let opcode = self.read_pc_memory(0);
        match opcode {
            opcode::ADD => self.exec_add(),
            opcode::STORE_HASH => self.exec_store_hash(),
            opcode::LOAD_HASH => self.exec_load_data(),
            opcode::SEND => self.exec_send(),
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
            opcode::STORE_HASH, 3,5,
            0x04030201, 0x00000005]).ok().unwrap();
        match vm.step() {
            Ok(Some(Request::StoreData{key, value})) => {
                assert_eq!(value, [1,2,3,4,5].to_vec());
                assert_eq!(key, [ // b2sum of the value
                    0x6b, 0x4c, 0x45, 0xfb, 0x95, 0x47, 0xe1, 0x9c,
                    0x90, 0x85, 0x16, 0x92, 0x76, 0x4f, 0x39, 0xfe,
                    0x92, 0x7a, 0x8c, 0xe7, 0x29, 0x5d, 0xd1, 0x5c,
                    0x8e, 0x15, 0xbf, 0xd7, 0x8d, 0xd7, 0x53, 0xc4,
                    0xbc, 0xc5, 0x7a, 0xa9, 0x29, 0xd4, 0x39, 0x4e,
                    0x62, 0x18, 0xae, 0x8f, 0xd0, 0xb1, 0xc8, 0xbf,
                    0x5f, 0x99, 0x10, 0xb9, 0xbd, 0xd2, 0x07, 0xc6,
                    0x02, 0xe0, 0x6c, 0x30, 0x13, 0x21, 0xa0, 0x23,
                    ].to_vec());
            }
            _ => { panic!("Expected a store request") }
        }
    }
    
    #[test]
    fn send() {
        let mut vm = Vm::new(&[
            opcode::SEND, /* recipient start = */ 6, /*message start = */ 4, /*message bytes = */ 8,
            0x04030201, 0x08070605, 
            // recipient
            0x11111111, 0x22222222, 0x33333333, 0x44444444,
            0x55555555, 0x66666666, 0x77777777, 0x88888888
            ]).ok().unwrap();
        match vm.step() {
            Ok(Some(Request::Send{recipient, message})) => {
                assert_eq!(recipient.len(), 32);
                assert_eq!(recipient[0], 0x11);
                assert_eq!(recipient[31], 0x88);
                assert_eq!(message, [1,2,3,4,5,6,7,8].to_vec());
            }
            _ => { panic!("Expected a store request") }
        }
    }
}
