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
    if count==0 {
        return Vec::new();
    }
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

fn add_name_resolutions(parent_name_resolutions: &HashMap<String, u64>, names: Vec<String>) -> HashMap<String, u64> {
    let definition_positions = dense_tree_positions(names.len());
    let mut name_resolutions = HashMap::new();
    for (name, pos) in parent_name_resolutions.iter() {
        name_resolutions.insert(name.clone(), add_initial_step(*pos, 1));
    }

    for (definition_position, name) in definition_positions.into_iter().zip(names.into_iter()) {
        name_resolutions.insert(name, add_initial_step(definition_position, 0));
    }
    name_resolutions
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
    let definition_tree = build_into_dense_tree(definition_exprs);
    let name_resolutions = add_name_resolutions(parent_name_resolutions, names);
    
    Ok((definition_tree, name_resolutions))
}

fn add_argument_name_resolutions(arguments: &Node, name_resolutions: &HashMap<String, u64>) -> Result<HashMap<String, u64>, String> {
    let args: Vec<String> = if let Node::Parent(args) = arguments {
        args.iter()
            .map(|arg| arg.as_symbol()
                .map(|name| name.to_string())
                .ok_or_else(|| "Argument name should be a symbol".to_string()))
            .collect::<Result<Vec<String>, String>>()?
    } else {
        return Err("Arguments to a lambda should be a list".to_string());
    };

    Ok(add_name_resolutions(name_resolutions, args))
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
        Node::List(children) => {
            vec_to_tree(children.iter().map(|child| compile_node(child, name_resolutions)).collect::<Result<Vec<Noun>, String>>()?)
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
                    } else if function_name == "let" { // (let ((x 10) (y 20)) (add x y))
                        if children.len() != 3 {
                            return Err("Malformed `let` expression".to_string());
                        }
                        let (bindings_evaluator, extended_name_resolutions) = add_bindings(&children[1], name_resolutions)?;
                        Noun::new_cell(Noun::from_u8(opcode::DEFINE),
                            Noun::new_cell(bindings_evaluator, compile_node(&children[2], &extended_name_resolutions)?))
                    } else if function_name == "lambda" { // (lambda (x y) (add x y))
                        if children.len() != 3 {
                            return Err("Malformed `lambda` expression".to_string());
                        }

                        // The scope's `name_resolutions` are going to be buried two levels down when this is called.
                        // First it is paired up with the code...
                        let extended_name_resolutions = add_name_resolutions(name_resolutions, vec![]); // We could potentially give name to the code
                        // ...then it is paired up with the arguments.
                        let extended_name_resolutions = add_argument_name_resolutions(&children[1], &extended_name_resolutions)?;

                        let lambda_body = compile_node(&children[2], &extended_name_resolutions)?;
                        Noun::new_cell(
                            Noun::new_cell(Noun::from_u8(opcode::LITERAL), lambda_body),
                            Noun::new_cell(Noun::from_u8(opcode::AXIS), Noun::from_u8(1)) // Copy everything in scope into the lambda
                        )
                    } else { // function call
                        if let Some(position) = name_resolutions.get(function_name) {
                            println!("Function call {}", function_name);
                            // The rest of the children are the arguments. That must be turned into a tree.
                            let arg_maker = build_into_dense_tree(children.iter()
                                .skip(1) // Skip the function name itself
                                .map(|arg| compile_node(arg, name_resolutions))
                                .collect::<Result<Vec<Noun>, String>>()?);
                            
                            let env_maker = Noun::new_cell(arg_maker, Noun::new_cell(Noun::from_u8(opcode::AXIS), Noun::from_u64_compact(*position)));
                            // The environment is of the format [args [lambda_code lambda_ctx]] 
                            Noun::new_cell(Noun::from_u8(opcode::CALL), Noun::new_cell(Noun::from_u8(6), env_maker))
                        } else {
                            return Err(format!("Unknown function `{}` called", function_name));
                        }
                    }
                }
                _ => {
                    return Err("Expected a function call-like token".to_string());
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
        compile_and_eval("[#33 #66]", (vec![0x33], vec![0x66])); 
    }

    #[test]
    fn is_cell() {
        compile_and_eval("(is_cell #2244)", 1);
        compile_and_eval("(is_cell [#2244 #33])", 0);
    }

    #[test]
    fn iff() {
        compile_and_eval("(if #00 #33 #44)", 0x33);
    }

    #[test]
    fn shape() {
        compile_and_eval("(shape [#665544332211 [#33 #44]])", (6, (1, 1)));
        compile_and_eval("[(shape #11223344) (shape #1122)]", (4, 2));
    }

    #[test]
    fn let_simple() {
        compile_and_eval("(let ((x #45)) x)", 0x45);
        compile_and_eval("(let ((x #45) (y #67)) (equal x y))", 1);
        compile_and_eval("(let ((x #45) (y #67) (z #21)) (add x z))", 0x66);
        compile_and_eval("(let ((x #10)) (add x (let ((y #21)) (add x y))))", 0x41);
    }

    #[test]
    fn lambda_simple() {
        // Just make a lambda and call it
        compile_and_eval("(let ((x #45) (y (lambda (a) (add a #01)))) (y x))", 0x46);
        // Set a lambda into a variable then call that
        compile_and_eval("(let ((x #45) (y (lambda (a) (add a #01)))) (let ((z y)) (z x)))", 0x46);
        // Make sure that variables in scope (x, specifically) are captured properly, even when called outside that scope
        compile_and_eval("(let ((f (let ((x #05) (y #03)) (lambda (z) (add x z))))) (f #04))", 0x09);
    }

    #[test]
    fn guessing_game() {
        compile_and_eval(r#"
            (let ((answer #42))
              (let ((handle_guess (lambda (g) 
                 (if (less g answer) "too low" (if (less answer g) "too high" "right")))))
                 [(handle_guess #33) (handle_guess #42) (handle_guess #55)]))
            "#, (&b"too low"[..], &b"right"[..], &b"too high"[..]));
    }
}
