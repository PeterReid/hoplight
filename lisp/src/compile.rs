use std::collections::HashMap;
use vm::Noun;
use vm::opcode;
use crate::tree::parse;


use crate::tree::Node;

fn native_opcode_for_name(name: &str) -> Option<(u8, usize)> {
    Some(match name {
        "random" => (opcode::RANDOM, 0),
        "is_cell" => (opcode::IS_CELL, 1),
        "hash" => (opcode::HASH, 1),
        "shape" => (opcode::SHAPE, 1),
        "if" => (opcode::IF, 3),
        "equal" => (opcode::IS_EQUAL, 2),
        _ => { return None; }
    })
}

fn vec_to_tree(xs: Vec<Noun>) -> Noun {
    let mut iter = xs.into_iter().rev();
    let mut ret = iter.next().expect("vec_to_tree cannot take an empty list");

    for node in iter {
        ret = Noun::new_cell(node, ret);
    } 

    ret
}

fn as_literal(x: &Node) -> Result<Noun, String> {
    Ok(match x {
        Node::Literal(bs) => Noun::from_vec(bs.clone()),
        Node::Symbol(x) => { return Err(format!("A symbol ({}) cannot be part of a literal", x)); },
        Node::Parent(children) => {
            if children.is_empty() {
                return Err("An empty list cannot occur in a literal".to_string());
            }
            let children_literals: Vec<Noun> = children.iter().map(as_literal).collect::<Result<Vec<Noun>, String>>()?;
            vec_to_tree(children_literals)
        }
    })
}

fn compile_node(node: &Node, name_resolutions: &HashMap<String, usize>) -> Result<Noun, String> {
    Ok(match node {
        Node::Symbol(variable_name) => {
            let position = name_resolutions.get(variable_name).ok_or_else(|| format!("Unresolved variable name: {}", variable_name))?;
            Noun::new_cell(Noun::from_u8(opcode::AXIS), Noun::from_usize_compact(*position))
        },
        Node::Literal(bs) => {
            Noun::new_cell(Noun::from_u8(opcode::LITERAL), Noun::from_vec(bs.clone()))
        }
        Node::Parent(children) => {
            let mut children_iter = children.iter();
            let first = children_iter.next().ok_or_else(|| "Tried to compile empty parent node ()".to_string())?;
            match first {
                Node::Symbol(function_name) => {
                    if let Some((native_opcode, expected_argc)) = native_opcode_for_name(function_name) {
                        if children.len() != expected_argc + 1 {
                            return Err(format!("Wrong number of parameters for '{}'. Expected {}, got {}.",
                                function_name, expected_argc, children.len()-1));
                        }
                        let mut compiled_args: Vec<Noun> = children_iter
                            .map(|arg| compile_node(arg, name_resolutions))
                            .collect::<Result<Vec<Noun>, String>>()?;
                        compiled_args.insert(0, Noun::from_u8(native_opcode));
                        vec_to_tree(compiled_args)
                    } else {
                        return Err("todo".to_string());
                    }
                }
                _ => {
                    // This is the beginning of a tree-type literal, apparently, not a function call
                    Noun::new_cell(
                        Noun::from_u8(opcode::LITERAL), 
                        vec_to_tree(children.iter().map(as_literal).collect::<Result<Vec<Noun>, String>>()?))
                }
            }
        }
       
    })
}

// (let ((x 4)
//       (y 9)
//       (z (lambda (z) (concat x y z))))
//      (z 10))

pub fn compile(code: &str) -> Result<Noun, String> {
    let ast = parse(code)?;
    println!("Compiled to {:?}", ast);
    // It seems like we need a final pass that resolves AXIS references for symbols to their actual places
    let x = HashMap::new();
    compile_node(&ast, &x)
}

#[cfg(test)]
mod test {
    use super::compile;
    use vm::AsNoun;
    use vm::Noun;

    fn compile_and_eval<E: AsNoun>(code: &str, expected: E) {
        let code_noun = compile(code).expect("compile failed");
        println!("Code: {:?}", code_noun);
        let subject_and_code = Noun::new_cell(Noun::from_u8(0), code_noun);
        let ret = vm::eval_simple(subject_and_code);

        assert_eq!(ret, expected.as_noun())
    }

    #[test]
    fn literal() {
        compile_and_eval("#33", vec![0x33]); 
        compile_and_eval("#33", vec![0x33]); 
    }

    #[test]
    fn is_cell() {
        compile_and_eval("(is_cell #2244)", 1);
        compile_and_eval("(is_cell (#2244 #33))", 0);
    }

    #[test]
    fn iff() {
        compile_and_eval("(if #00 #33 #44)", 0x33);
    }

    #[test]
    fn shape() {
        compile_and_eval("(shape (#665544332211 (#33 #44)))", (6, (1, 1)));
    }
}
