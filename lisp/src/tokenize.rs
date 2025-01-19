#[derive(Eq, PartialEq, Debug)]
pub enum Token {
    Literal(Vec<u8>),
    Symbol(String),
    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket
}

fn is_symbol_initial(x: char) -> bool {
    (x >= 'a' && x <= 'z') || (x >= 'A' && x <= 'Z') || x == '_'
}
fn is_symbol_continuation(x: char) -> bool {
    is_symbol_initial(x) || (x >= '0' && x <= '9')
}
fn as_string_literal_char(c: char) -> Option<u8> {
    if (c as u32) >= 32 && (c as u32) < 127 {
        Some(c as u8)
    } else {
        None
    }
}
fn hex_value(b: char) -> Option<u8> {
    if b >= '0' && b <= '9' {
        Some((b as u8) - ('0' as u8))
    } else if b >= 'a' && b <= 'f' {
        Some((b as u8) - ('a' as u8) + 10)
    } else if b >= 'A' && b <= 'F' {
        Some((b as u8) - ('A' as u8) + 10)
    } else {
        None
    }
}
fn byte_from_hex_chars(b0: char, b1: char) -> Option<u8> {
    if let (Some(v0), Some(v1)) = (hex_value(b0), hex_value(b1)) {
        Some((v0<<4) | v1)
    } else {
        None
    }
}
fn or_zero(x: Option<(usize, char)>) -> char {
    x.map(|x| x.1).unwrap_or('\0')
}
pub fn tokenize(code: &str) -> Result<Vec<Token>, String>  {
    let mut tokens = Vec::new();
    let mut remaining_code = code.chars().enumerate().peekable();
    'char_consumer: loop {
        let (idx, next_char) = match remaining_code.next() {
            None => { break 'char_consumer; }
            Some(next_char) => next_char
        };

        match next_char {
            ' ' | '\r' | '\n' | '\t' => { continue; }
            '(' => {
                tokens.push(Token::OpenParen);
            }
            ')' => {
                tokens.push(Token::CloseParen);
            }
            '[' => {
                tokens.push(Token::OpenBracket);
            }
            ']' => {
                tokens.push(Token::CloseBracket);
            }
            '#' => { // Hex-encoded literal
                let mut literal = Vec::new();
                while is_symbol_continuation(or_zero(remaining_code.peek().map(|x| *x))) {
                    let (digit_idx, tens_digit) = remaining_code.next().unwrap();
                    let ones_digit = or_zero(remaining_code.next());
                
                    if let Some(byte) = byte_from_hex_chars(tens_digit, ones_digit) {
                        literal.push(byte);
                    } else {
                        return Err(format!("Hexadecimal literal malformed at character {}", digit_idx));
                    }
                } 
                tokens.push(Token::Literal(literal))
            }
            '\"' => { // String literal. The only whitespace allowed is a space.
                let mut literal = Vec::new();
                loop {
                    let c = match remaining_code.next() {
                        Some((_, c)) => c,
                        None => { return Err(format!("String beginning at character position {} did not end.", idx)); }
                    };
                    if c == '\\' { // Handle escape sequences
                        let (escape_char_idx, escape_char) = remaining_code.next().unwrap_or((0, '\0'));
                        match escape_char {
                            'n' => { literal.push(b'\n'); }
                            'r' => { literal.push(b'\r'); }
                            't' => { literal.push(b'\t'); }
                            '\"' => { literal.push(b'\"'); }
                            'x' => {
                                let tens_digit = or_zero(remaining_code.next());
                                let ones_digit = or_zero(remaining_code.next());
                                if let Some(byte) = byte_from_hex_chars(tens_digit, ones_digit) {
                                    literal.push(byte);
                                } else {
                                    return Err(format!("Hexadecimal-encoded byte inside a string at character {}", escape_char_idx));
                                }
                            }
                            _ => {
                                return Err(format!("Invalid escape character following \\ at character {}", escape_char_idx));
                            }
                        }
                    } else if c == '\"' {
                        break;
                    } else if let Some(b) = as_string_literal_char(c) {
                        literal.push(b);
                    }
                }
                tokens.push(Token::Literal(literal));
            }
            ';' => { // Comment, terminated by a line break
                while remaining_code.next().unwrap_or((0, '\n')).1 != '\n' {

                }
            }
            x if is_symbol_initial(x) => {
                let mut symbol_name = String::from(x);
                while remaining_code.peek().map(|(_, continuation)| is_symbol_continuation(*continuation)) == Some(true) {
                    symbol_name.push(remaining_code.next().unwrap().1);
                }
                tokens.push(Token::Symbol(symbol_name));
            }
            _ => {
                return Err(format!("Invalid chararacter at position {}", idx));
            }
        }
    }
    
    Ok(tokens)
}
#[cfg(test)]
mod test {
    use super::{tokenize, Token};

    #[test]
    fn tokenize1() {
        assert_eq!(tokenize("(foo)"), Ok(vec![Token::OpenParen, Token::Symbol("foo".to_string()), Token::CloseParen]));
    }
    #[test]
    fn brackets() {
        assert_eq!(tokenize("[\"test\"]"), Ok(vec![Token::OpenBracket, Token::Literal(b"test".to_vec()), Token::CloseBracket]));
    }
    #[test]
    fn tokenize_str() {
        assert_eq!(tokenize("(\"blue?\")"), Ok(vec![Token::OpenParen, Token::Literal(b"blue?".to_vec()), Token::CloseParen]));
    }
    #[test]
    fn tokenize_str_hex_escape() {
        assert_eq!(tokenize("(\"\\x01\\x02\\xff\")"), Ok(vec![Token::OpenParen, Token::Literal([1,2,255].to_vec()), Token::CloseParen]));
    }
    #[test]
    fn tokenize_str_basic_escape() {
        assert_eq!(
            tokenize("(\"CR: \\r LF: \\n TAB: \\t QUOTE: \\\"\")"), 
            Ok(vec![Token::OpenParen, Token::Literal(b"CR: \r LF: \n TAB: \t QUOTE: \"".to_vec()), Token::CloseParen]));
    }
    #[test]
    fn tokenize_hex() {
        assert_eq!(tokenize("#1234ffbc #3456"), Ok(vec!(Token::Literal([0x12, 0x34, 0xff, 0xbc].to_vec()), Token::Literal([0x34, 0x56].to_vec()))));
    }
    #[test]
    fn tokenize_comment() {
        assert_eq!(tokenize("foo ; comment here\nbar"), Ok(vec!(Token::Symbol("foo".to_string()), Token::Symbol("bar".to_string()))));
    }
}
