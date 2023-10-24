use chacha::{ChaCha, KeyStream};
use vm::{eval, SideEffectEngine, Noun};
use std::collections::HashMap;
use std::iter::Peekable;
use std::io;
use std::io::BufRead;
use std::io::Write;

struct TestSideEffectEngine {
    storage: HashMap<Vec<u8>, Vec<u8>>,
    rng: ChaCha,
}

impl TestSideEffectEngine {
    fn new() -> TestSideEffectEngine {
        TestSideEffectEngine {
            storage: HashMap::new(),
            rng: ChaCha::new_chacha20(&[1u8; 32], &[0u8; 8]),
        }
    }
}

impl SideEffectEngine for TestSideEffectEngine {
    fn nearest_neighbor(&mut self, _near: &[u8; 32]) -> [u8; 32] {
        [0u8; 32]
    }
    fn random(&mut self, dest: &mut [u8]) {
        for b in dest.iter_mut() {
            *b = 0;
        }
        self.rng.xor_read(dest).expect("RNG end reached");
    }
    fn load(&mut self, key: &[u8]) -> Option<Vec<u8>> {
        self.storage.get(key).cloned()
    }
    fn store(&mut self, key: &[u8], value: &[u8]) {
        self.storage.insert(key.into(), value.into());
    }
    fn send(&mut self, _destination: &[u8; 32], _message: &[u8], _local_cost: u64) {}
    fn secret(&self) -> &[u8; 32] {
        b"this is a thirty-two byte secret"
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Token {
    OpenBracket,
    CloseBracket,
    Atom(Vec<u8>),
    SyntaxError,
}

struct Tokenizer<T: Iterator<Item = u8>> {
    byte_stream: Peekable<T>
}

impl<T: Iterator<Item = u8>> Tokenizer<T> {
    fn new(byte_stream: T) -> Self {
        Tokenizer {
            byte_stream: byte_stream.peekable(),
        }
    }
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0' ..= b'9' => Some(b - b'0'),
        b'A' ..= b'F' => Some(b - b'A' + 10),
        b'a' ..= b'f' => Some(b - b'a' + 10),
        _ => None
    }
}

fn from_decimal(b: u8) -> Option<u8> {
    match b {
        b'0' ..= b'9' => Some(b - b'0'),
        _ => None
    }
}



impl<T: Iterator<Item = u8>> Iterator for Tokenizer<T> {
    type Item = Token;
    
    fn next(&mut self) -> Option<Token> {
        // Skip any whitespace
        while self.byte_stream.peek().map(|x| x.is_ascii_whitespace()) == Some(true) {
            self.byte_stream.next();
        }
        
        let first = self.byte_stream.next()?;
        Some(match first {
            b'[' => Token::OpenBracket,
            b']' => Token::CloseBracket,
            b'x' => {
                let mut atom_bytes = Vec::new();
                
                while let Some(high_half) = self.byte_stream.peek().and_then(|b| from_hex(*b)) {
                    let _ = self.byte_stream.next();
                    if let Some(low_half) = self.byte_stream.next().and_then(|b| from_hex(b)) {
                        atom_bytes.push((high_half<<4) | low_half);
                    } else {
                        return Some(Token::SyntaxError)
                    }
                }
                
                Token::Atom(atom_bytes)
            }
            b'0' ..= b'9' => {
                let mut val = (first - b'0') as usize;
                
                while let Some(digit) = self.byte_stream.peek().and_then(|b| from_decimal(*b)) {
                    let _ = self.byte_stream.next();
                    val = match val.checked_mul(10).and_then(|x| x.checked_add(digit as usize)) {
                        Some(val) => val,
                        None => { return Some(Token::SyntaxError) },
                    }
                }
                
                if val != 0 {
                    Token::Atom( Noun::from_usize_compact(val).into_vec().unwrap() )
                } else {
                    Token::Atom( vec![ 0 ])
                }
            }
            _ => Token::SyntaxError
        })
    }
}

#[derive(Debug)]
struct ParseError;

fn parse<T: Iterator<Item = Token>> (tokens: &mut Peekable<T>) -> Result<Noun, ParseError> {
    match tokens.next().ok_or(ParseError)? {
        Token::OpenBracket => {
            let mut subitems = Vec::new();
            while tokens.peek() != Some(&Token::CloseBracket) {
                subitems.push(parse(tokens)?);
            }
            let _close_bracket = tokens.next();
            
            if subitems.len() < 2 {
                return Err(ParseError);
            }
            subitems.reverse();
            let mut subitems_iter = subitems.into_iter();
            let right = subitems_iter.next().ok_or(ParseError)?;
            let left = subitems_iter.next().ok_or(ParseError)?;
            let mut cell = Noun::new_cell(left, right);
            
            while let Some(left) = subitems_iter.next() {
                cell = Noun::new_cell(left, cell);
            }
            Ok(cell)
        },
        Token::Atom(bs) => Ok(Noun::from_vec(bs)),
        Token::SyntaxError => {
            Err(ParseError)
        },
        Token::CloseBracket => {
            Err(ParseError)
        }
    }
    
}

fn main() {
    let mut engine = TestSideEffectEngine::new();
    
    print!("> ");
    let _ = io::stdout().flush();
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => {
                return;
            }
        };
        
        let mut tokens = Tokenizer::new(line.trim().as_bytes().iter().map(|x| *x)).peekable();
        match parse(&mut tokens) {
            Ok(expr) => {
                match eval(expr, &mut engine, 1000000) {
                    Ok(result) => { println!("{:?}", result) },
                    Err(err) => { println!("Error: {:?}", err); }
                }
            }
            Err(e) => {
                println!("{:?}", e);
            }
        }
        print!("> ");
        let _ = io::stdout().flush();
    }
}
