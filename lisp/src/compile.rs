use std::collections::HashMap;
use vm::Noun;
use vm::opcode;
use crate::tree::parse;


use crate::tree::Node;

fn native_opcode_for_name(name: &str) -> Option<(u8, usize)> {
    Some(match name {
        "random" => (opcode::RANDOM, 1),
        "is_cell" => (opcode::IS_CELL, 1),
        "hash" => (opcode::HASH, 1),
        "shape" => (opcode::SHAPE, 1),
        "if" => (opcode::IF, 3),
        "equal" => (opcode::IS_EQUAL, 2),
        "store_by_hash" => (opcode::STORE_BY_HASH, 1),
        "retrieve_by_hash" => (opcode::RETRIEVE_BY_HASH, 1),
        "store_by_key" => (opcode::STORE_BY_KEY, 2),
        "retrieve_by_key" => (opcode::RETRIEVE_BY_KEY, 1),
        "generate_keypair" => (opcode::GENERATE_KEYPAIR, 0),
        "encrypt" => (opcode::ENCRYPT, 2),
        "decrypt" => (opcode::DECRYPT, 2),
        "exucrypt" => (opcode::EXUCRYPT, 2),
        "add" => (opcode::ADD, 2),
        "invert" => (opcode::INVERT, 1),
        "xor" => (opcode::XOR, 2),
        "less" => (opcode::LESS, 2),
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


// [a,b,c,d,e,f,g] is transformed to:
//
//            .
//          /   \
//      .          .
//   /     \     /    g
//  a b   c d   e f   7
//  8 9  10 11 12 13
fn build_into_dense_tree(mut ns: Vec<Noun>) -> Noun {
    assert!(!ns.is_empty());
    let mut round = 0;
    while ns.len() > 1 {
        println!("at top, ns = {:?}", ns);

        round += 1;
        assert!(round < 10);
        let mut packed_ns = Vec::new();
        let mut ns_iter = ns.into_iter();
        'pairing_loop: loop {
            match (ns_iter.next(), ns_iter.next()) {
                (Some(left), Some(right)) => { packed_ns.push(Noun::new_cell(left, right)) },
                (Some(left), None) => { packed_ns.push(left); },
                _ => { break 'pairing_loop; }
            }
        }
        ns = packed_ns;
    }
    ns.into_iter().next().unwrap()
}
fn dense_tree_positions(count: usize) -> Vec<u64> {
    let count = count as u64;
    let max_level_needed = count.ilog2() + 1;
    let spots_in_max_level = 1<<max_level_needed;
    let extra_spaces_in_max_level = spots_in_max_level - count;
    let nouns_at_max_level = spots_in_max_level - extra_spaces_in_max_level*2;
    let nouns_at_level_above = count - nouns_at_max_level;
    let first_noun_at_max_level = spots_in_max_level;
    let first_noun_at_level_above = first_noun_at_max_level - nouns_at_level_above;

    let nouns_at_max_level = first_noun_at_max_level..first_noun_at_max_level+nouns_at_max_level;
    let nouns_at_level_above = first_noun_at_level_above..first_noun_at_level_above+nouns_at_level_above;

    nouns_at_max_level.chain(nouns_at_level_above).collect()
}
#[test]
fn dense_tree() {
    // The example in the comment of build_into_dense_tree:
    let ns: Vec<Noun> = (b'a' ..= b'g').map(|c| Noun::from_u8(c)).collect();
    let positions = dense_tree_positions(ns.len());
    let tree = build_into_dense_tree(ns);

    assert_eq!(positions, vec![8,9,10,11,12,13,7]);
    assert_eq!(b'd', tree.into_cell().unwrap().0.into_cell().unwrap().1.into_cell().unwrap().1.as_byte().unwrap());
}
#[test]
fn dense_tree_full() {
    let positions = dense_tree_positions(16);
    assert_eq!(positions, (16..32).collect::<Vec<u64>>());
}


fn add_initial_step(axis_placement: u64, initial_step: u64) -> u64 {
    assert!(initial_step==0 || initial_step==1);
    // The placement has a most-significant "1" to start, and the 0s for lefts and 1s for rights.
    // We need to scoot that most-signficant "1" leftward one bit, and add our new step right after it.

    let leading_one_position = axis_placement.ilog2();
    let later_steps_mask = axis_placement & ((1 << leading_one_position)-1);
    
    (1 << (leading_one_position+1)) | (initial_step << leading_one_position) | (axis_placement & later_steps_mask)
}
#[test]
fn initial_step_left() {
    assert_eq!(add_initial_step(0b1001, 0), 0b10001);
    assert_eq!(add_initial_step(0b1, 1), 0b11);
}

fn add_bindings(bindings_list: &Node, parent_name_resolutions: &HashMap<String, u64>) -> Result<(Noun, HashMap<String, u64>), String>{
    let bindings = if let Node::Parent(children) = bindings_list {
        children
    } else {
        return Err("Expected first argument of `let` expression to be a list of variables to introduce.".to_string());
    };

    let mut definition_exprs: Vec<Noun> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for binding in bindings.iter() {
        if let Node::Parent(name_and_expr) = binding {
            if name_and_expr.len() != 2 { return Err("Malformed (name expression) pair in `let` expression".to_string()); }
            let name = name_and_expr[0].as_symbol()
                .ok_or_else(|| "Expected symbol as the introduced variable name in `let` expression".to_string())?;
            definition_exprs.push(compile_node(&name_and_expr[1], parent_name_resolutions)?);
            names.push(name.to_string());
        } else {
            return Err("Expected each item of first argument of `let` expression to be a (name expression) pair".to_string());
        }
    }
    let definition_positions = dense_tree_positions(definition_exprs.len());
    let definition_tree = build_into_dense_tree(definition_exprs);

    let new_subject_builder = Noun::new_cell(
        // The left of the new subject is just the old subject
        Noun::new_cell(Noun::from_u8(opcode::AXIS), Noun::from_u8(1)),
        definition_tree
    );

    let mut name_resolutions = HashMap::new();
    for (name, pos) in parent_name_resolutions.iter() {
        name_resolutions.insert(name.clone(), add_initial_step(*pos, 0));
    }

    for (definition_position, name) in definition_positions.into_iter().zip(names.into_iter()) {
        name_resolutions.insert(name, add_initial_step(definition_position, 1));
    }
    Ok((new_subject_builder, name_resolutions))
}

fn compile_node(node: &Node, name_resolutions: &HashMap<String, u64>) -> Result<Noun, String> {
    Ok(match node {
        Node::Symbol(variable_name) => {
            let position = name_resolutions.get(variable_name).ok_or_else(|| format!("Unresolved variable name: {}", variable_name))?;
            Noun::new_cell(Noun::from_u8(opcode::AXIS), Noun::from_u64_compact(*position))
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
                    } else if function_name == "let" { // (let ((x 10) (y 20)) (x + y))
                        if children.len() != 3 {
                            return Err("Malformed `let` expression".to_string());
                        }
                        let (bindings_evaluator, extended_name_resolutions) = add_bindings(&children[1], name_resolutions)?;
                        Noun::new_cell(Noun::from_u8(opcode::COMPOSE),
                            Noun::new_cell(bindings_evaluator, compile_node(&children[2], &extended_name_resolutions)?))
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

    #[test]
    fn let_simple() {

        compile_and_eval("(let ((x #45)) x)", 0x45);
    }
}
