use std::collections::HashMap;
use identity::Identity;

#[derive(Debug)]
pub struct ExpectedPacketSet {
    inner: HashMap<u64, Vec<ExpectedPacket>>,
    empty: [ExpectedPacket; 0],
}

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub struct ExpectedPacket {
    pub stream_with: Identity,
    pub stream_key: [u8; 32],
    pub packet_number: u64,
}

impl ExpectedPacketSet {
    pub fn new() -> ExpectedPacketSet {
        ExpectedPacketSet {
            inner: HashMap::new(),
            empty: []
        }
    }
    
    pub fn add(&mut self, packet: ExpectedPacket, identifier: u64) {
        if let Some(list) = self.inner.get_mut(&identifier) {
            list.push(packet);
            return;
        }
        
        self.inner.insert(identifier, vec![packet]);
    }
    
    pub fn remove(&mut self, packet: &ExpectedPacket, identifier: u64) {
        let emptied = if let Some(list) = self.inner.get_mut(&identifier) {
            if let Some(idx) = list.iter().position(|x| *x == *packet) {
                list.swap_remove(idx);
            }
            
            list.len() == 0
        } else {
            false
        };
        
        if emptied {
            self.inner.remove(&identifier);
        }
    }

    pub fn iter(&self, identifier: u64) -> ::std::slice::Iter<ExpectedPacket> {
        if let Some(list) = self.inner.get(&identifier) {
            list.iter()
        } else {
            self.empty.iter()
        }
    }
    
}