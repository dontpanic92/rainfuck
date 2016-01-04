use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;

mod interpreter;
mod jit_compiler;

fn read_file(path: &String) -> String{
    let mut code_buffer = String::new();

    let file = match File::open(path) {
        Err(reason) => {
            panic!("Could not open {}: {}", path, reason);
        },
        Ok(file) => file
    };

    let mut fin = BufReader::new(file);
    fin.read_to_string(&mut code_buffer).unwrap();
    code_buffer
}

fn main() {
    let mut j = jit_compiler::JitCompiler::new();
    j.compile("");

    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        interpreter::interpret(&read_file(&args[1]));
    } else {
        println!("Usage: {} program.bf", args[0]);
    }
}
