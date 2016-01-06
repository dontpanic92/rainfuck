use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;

mod interpreter;
mod jit_compiler_x64;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

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
    let args: Vec<String> = env::args().collect();
    if args.len() > 2 && args[1] == "--no-jit" {
        interpreter::interpret(&read_file(&args[2]));
    } else if args.len() > 1 {
        let mut j = jit_compiler_x64::JitCompiler::new();
        j.compile_and_run(&read_file(&args[1]));
    } else {
        println!("Rainfuck version {}\n", VERSION);
        println!("Usage: {} [--no-jit] program.bf", args[0]);
    }
}
