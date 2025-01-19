use crate::tokenize;

use tokenize::Token;

#[derive(Debug, Eq, PartialEq)]
pub enum Node {
    Parent(Vec<Node>),
    Symbol(String),
    Literal(Vec<u8>),
    List(Vec<Node>),
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

enum ParseSome {
    EndBrackets,
    EndParens,
    Child(Node),
}
impl ParseSome {
    fn for_parens(self) -> Result<Option<Node>, String> {
        match self {
            ParseSome::EndParens => Ok(None),
            ParseSome::Child(n) => Ok(Some(n)),
            ParseSome::EndBrackets => Err("Unexpected closing bracket".to_string())
        }
    }
    fn for_brackets(self) -> Result<Option<Node>, String> {
        match self {
            ParseSome::EndBrackets => Ok(None),
            ParseSome::Child(n) => Ok(Some(n)),
            ParseSome::EndParens => Err("Unexpected closing parentheses".to_string())
        }
    }
}

fn parse_some<T: Iterator<Item = Token>>(tokens: &mut T) -> Result<ParseSome, String> {
    let token = match tokens.next() {
        Some(token) => token,
        None => { return Err("Unexpected end".to_string()); }
    };
    match token {
        Token::OpenParen => {
            let mut children = Vec::new();
            while let Some(child) = parse_some(tokens)?.for_parens()? {
                children.push(child);
            }
            return Ok(ParseSome::Child(Node::Parent(children)));
        },
        Token::CloseParen => {
            return Ok(ParseSome::EndParens);
        }
        Token::OpenBracket => {
            let mut children = Vec::new();
            while let Some(child) = parse_some(tokens)?.for_brackets()? {
                children.push(child);
            }
            return Ok(ParseSome::Child(Node::List(children)));
        },
        Token::CloseBracket => {
            return Ok(ParseSome::EndBrackets);
        }
        Token::Symbol(x) => {
            return Ok(ParseSome::Child(Node::Symbol(x)));
        },
        Token::Literal(x) => {
            return Ok(ParseSome::Child(Node::Literal(x)));
        }
    }
}

pub fn parse(code: &str) -> Result<Node, String> {
    let tokens = tokenize::tokenize(code)?;
    let mut tokens_iter = tokens.into_iter();
    let root = if let ParseSome::Child(root) = parse_some(&mut tokens_iter)? { root } else { return Err("no node".to_string()) };
    
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
    #[test]
    fn parse2() {
        assert_eq!(
            parse("[#44 #88]").unwrap(),
            Node::List(vec![Node::Literal([0x44].to_vec()), Node::Literal([0x88].to_vec())])
        );
    }
}

