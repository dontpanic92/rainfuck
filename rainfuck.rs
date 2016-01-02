use std::io;
use std::io::prelude::*;
use std::io::BufReader;
use std::fs::File;
use std::env;

fn find_match<F: FnMut() -> char>(target_char: char, matched_char: char, next: &mut F) {
    let mut accu = 1;
    loop {
        match next() {
            c if c == matched_char => accu += 1,
            c if c == target_char => accu -= 1,
            _ => ()
        }
        if accu == 0 {
            break;
        }
    }
}

fn execute(code: &Vec<char>, pc: &mut usize, memory: &mut Vec<u8>, dc:&mut usize) {
    match code[*pc] {
        '>' => *dc += 1,
        '<' => *dc -= 1,
        '+' => memory[*dc] += 1,
        '-' => memory[*dc] -= 1,
        '.' => {
            print!("{}", memory[*dc] as char);
            io::stdout().flush().unwrap();
        },
        ',' => {
            let mut tmp_str = String::new();
            std::io::stdin().read_line(&mut tmp_str).unwrap();
            memory[*dc] = tmp_str.chars().next().unwrap() as u8;
        },
        '[' => if memory[*dc] == 0 {
            find_match(']', '[', &mut || {*pc += 1;code[*pc]});
        },
        ']' => if memory[*dc] != 0{
            find_match('[', ']', &mut || {*pc -= 1;code[*pc]});
        },
        _ => ()
    }

    *pc += 1
}

fn interpret_file(path:& String) {
    let mut memory: Vec<u8>= vec![0; 10000];
    let mut mem_pointer: usize = 0;
    let mut code_buffer = String::new();

    let file = match File::open(path) {
        Err(reason) => {
            println!("Could not open {}: {}", path, reason);
            return
        },
        Ok(file) => file
    };

    let mut fin = BufReader::new(file);

    fin.read_to_string(&mut code_buffer).unwrap();

    let code: Vec<char> = code_buffer.chars().collect();
    let mut code_pointer: usize = 0;
    loop {
        execute(&code, &mut code_pointer, &mut memory, &mut mem_pointer);
        if code_pointer >= code.len() {
            break;
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        interpret_file(&args[1]);
    } else {
        println!("Usage: {} program.bf", args[0]);
    }
}
