#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub struct IpAddressPort{
    pub address: [u8; 16],
    pub port: u16,
}
