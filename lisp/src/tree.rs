use crate::tokenize;

use tokenize::Token;

#[derive(Debug, Eq, PartialEq)]
pub enum Node {
    Parent(Vec<Node>),
    Symbol(String),
    Literal(Vec<u8>)
}

impl Node {
    pub fn as_symbol(&self) -> Option<&str> {
        if let Node::Symbol(name) = self {
            Some(name)
        } else {
            None
        }
    }
}

fn parse_some<T: Iterator<Item = Token>>(tokens: &mut T) -> Result<Option<Node>, String> {
    let token = match tokens.next() {
        Some(token) => token,
        None => { return Err("Unexpected end".to_string()); }
    };
    match token {
        Token::OpenParen => {
            let mut children = Vec::new();
            while let Some(child) = parse_some(tokens)? {
                children.push(child);
            }
            return Ok(Some(Node::Parent(children)));
        },
        Token::CloseParen => {
            return Ok(None);
        }
        Token::Symbol(x) => {
            return Ok(Some(Node::Symbol(x)));
        },
        Token::Literal(x) => {
            return Ok(Some(Node::Literal(x)));
        }
    }
}

pub fn parse(code: &str) -> Result<Node, String> {
    let tokens = tokenize::tokenize(code)?;
    let mut tokens_iter = tokens.into_iter();
    let root = parse_some(&mut tokens_iter)?.ok_or("no node".to_string())?;

    
    if tokens_iter.next().is_some() {
        return Err("Expected only one root node".to_string());
    }
    Ok(root)
}

#[cfg(test)]
mod test {
    use super::{parse, Node};

    #[test]
    fn parse1() {
        assert_eq!(
            parse("(concat x (concat #3344 #55))").unwrap(),
            Node::Parent(vec![
                Node::Symbol("concat".to_string()), 
                Node::Symbol("x".to_string()), 
                Node::Parent(vec![
                    Node::Symbol("concat".to_string()), 
                    Node::Literal([0x33, 0x44].to_vec()),
                    Node::Literal([0x55].to_vec())
                ])
            ])
        );
    }
}

