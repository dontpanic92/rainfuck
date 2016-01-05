extern crate libc;

extern {
    fn memcpy(dest: *mut libc::c_void, srouce: *mut libc::c_void, n: libc::size_t) -> *mut libc::c_void;
    fn putchar(ch: libc::c_int) -> libc::c_int;
    fn getchar() -> libc::c_int;
}

use std::mem;
use std::slice;
use std::collections::HashMap;

enum Reg{
    Rax,
    Rbx,
    Rcx,
    Rdx,
    Rdi,
    Rbp,
    Rsp
}

enum PanicReason {
    MemoryError,
    NotSupported
}

const DEFAULT_NUM_PAGES: usize = 2;
struct JitCode {
    machine_code: *mut u8,
    code_size: usize,
    buf_size: usize,
    page_size: usize,
    reloc_tbl: HashMap<usize, u64>
}

impl JitCode {
    fn new() -> JitCode {
        let mut jc = JitCode {
                machine_code: 0 as *mut u8,
                code_size: 0,
                page_size: 0,
                buf_size: 0,
                reloc_tbl: HashMap::new()};
        jc.expand_buffer();
        jc
    }

    fn expand_buffer(&mut self) {
        unsafe{
            let mut memory: *mut libc::c_void = 0 as *mut libc::c_void;

            let page_size = if self.page_size == 0 {
                    libc::sysconf(libc::_SC_PAGESIZE) as usize
                    //16
                } else {
                    self.page_size
                };

            let buf_size = if self.buf_size == 0 {
                    DEFAULT_NUM_PAGES * page_size
                } else {
                    self.buf_size * 2
                };

            match libc::posix_memalign(&mut memory, page_size, buf_size) {
                0 => (),
                _ => JitCode::panic(PanicReason::MemoryError)
            }

            libc::mprotect(memory, buf_size, libc::PROT_WRITE | libc::PROT_READ);

            if !self.machine_code.is_null() {
                memcpy(memory, mem::transmute(self.machine_code), self.buf_size);
                libc::free(mem::transmute(self.machine_code));
            }

            self.machine_code = mem::transmute(memory);
            self.buf_size = buf_size;
        }
    }

    fn check_buffer(&mut self, newcode_len: usize) {
        if self.code_size + newcode_len >= self.buf_size {
            JitCode::expand_buffer(self);
        }
    }

    fn emit_code(&mut self, machine_code: &[u8]) {
        self.check_buffer(machine_code.len());
        unsafe {
            for ch in machine_code {
                *self.machine_code.offset(self.code_size as isize) = *ch;
                self.code_size += 1;
            }
        }
    }

    fn emit_push_r(&mut self, op: Reg) {
        match op {
            Reg::Rbp => self.emit_code(&[0x55]),
            _ => JitCode::panic(PanicReason::NotSupported)
        }
    }

    fn emit_mov_rr(&mut self, op1: Reg, op2: Reg) {
        match (op1, op2) {
            (Reg::Rbp, Reg::Rsp) => self.emit_code(&[0x48, 0x89, 0xe5]),
            (Reg::Rsp, Reg::Rbp) => self.emit_code(&[0x48, 0x89, 0xEC]),
            (Reg::Rdx, Reg::Rdi) => self.emit_code(&[0x48, 0x89, 0xFA]),
            (Reg::Rdi, Reg::Rax) => self.emit_code(&[0x48, 0x89, 0xC7]),
            _ => JitCode::panic(PanicReason::NotSupported)
        }
    }

    fn emit_mov_ri(&mut self, op1: Reg, op2: u64) {
        let is32 = JitCode::imm_is_u32(op2);

        match (op1, is32) {
            (Reg::Rax, true) => self.emit_code(&[0x48, 0xC7, 0xC0]),
            (Reg::Rax, false) => self.emit_code(&[0x48, 0xB8]),
            (Reg::Rbx, true) => self.emit_code(&[0x48, 0xC7, 0xC3]),
            (Reg::Rbx, false) => self.emit_code(&[0x48, 0xBB]),
            (Reg::Rcx, true) => self.emit_code(&[0x48, 0xC7, 0xC1]),
            (Reg::Rcx, false) => self.emit_code(&[0x48, 0xB9]),
            (Reg::Rdi, true) => self.emit_code(&[0x48, 0xC7, 0xC7]),
            (Reg::Rdi, false) => self.emit_code(&[0x48, 0xBF]),
            _ => JitCode::panic(PanicReason::NotSupported)
        }

        if is32 {
            self.emit_code(JitCode::get_raw_slice(&(op2 as u32).to_le()));
        } else {
            self.emit_code(JitCode::get_raw_slice(&(op2).to_le()));
        }
    }

    fn emit_inc_r(&mut self, op: Reg) {
        match op {
            Reg::Rbx => self.emit_code(&[0x48, 0xFF, 0xC3]),
            Reg::Rcx => self.emit_code(&[0x48, 0xFF, 0xC1]),
            _ => JitCode::panic(PanicReason::NotSupported)
        }
    }

    fn emit_dec_r(&mut self, op: Reg) {
        match op {
            Reg::Rbx => self.emit_code(&[0x48, 0xFF, 0xCB]),
            Reg::Rcx => self.emit_code(&[0x48, 0xFF, 0xC9]),
            _ => JitCode::panic(PanicReason::NotSupported)
        }
    }

    fn emit_call_i(&mut self, op_dest: u64) {
        self.emit_code(&[0xe8]);
        self.reloc_tbl.insert(self.code_size, op_dest);
        self.emit_code(&[0x12, 0x34, 0x56, 0x78]);
    }

    fn emit_call_r(&mut self, op: Reg) {
        match op {
            Reg::Rbx => self.emit_code(&[0xFF, 0xD3]),
            Reg::Rcx => self.emit_code(&[0xFF, 0xD1]),
            _ => JitCode::panic(PanicReason::NotSupported)
        }
    }

    fn emit_pop_r(&mut self, op: Reg) {
        match op {
            Reg::Rbp => self.emit_code(&[0x5D]),
            _ => JitCode::panic(PanicReason::NotSupported)
        }
    }

    fn emit_ret(&mut self) {
        self.emit_code(&[0xC3]);
    }

    fn reloc(&mut self) {
        for (addr, dest) in &self.reloc_tbl {
            let code_addr = self.machine_code as u64 + *addr as u64;
            let offset = (*dest as i64) - (code_addr as i64) - 4;
            if !JitCode::imm_is_i32(offset) {
                panic!("Too far to call a subroutine, from 0x{:X} call 0x{:X}", code_addr, dest);
            }

            let le_offset = (offset as i32).to_le();
            let code = JitCode::get_raw_slice(&le_offset);
            unsafe {
                for i in 0..code.len() {
                    *self.machine_code.offset((*addr + i) as isize) = code[i];
                }
            }
        }
    }

    fn function(&mut self) -> *const u8 {
        self.reloc();
        unsafe {
            libc::mprotect(mem::transmute(self.machine_code), self.buf_size, libc::PROT_READ | libc::PROT_EXEC);
            //mem::transmute(self.machine_code)
            self.machine_code
        }
    }


    //Helper functions

    fn get_raw_slice<'a, T>(imm:&T) -> &'a [u8]{
        unsafe {
            slice::from_raw_parts(imm as *const T as *const u8, mem::size_of::<T>())
        }
    }

    fn imm_is_i32(imm: i64) -> bool {
        imm <= i32::max_value() as i64 && imm >= i32::min_value() as i64
    }

    fn imm_is_u32(imm: u64) -> bool{
        imm <= u32::max_value() as u64
    }

    fn panic(reason: PanicReason) {
        match reason {
            PanicReason::NotSupported
                => panic!("Not supported instruction."),
            PanicReason::MemoryError
                => panic!("Error allocating memory.")
        }
    }
}

impl Drop for JitCode {
    fn drop(&mut self) {
        unsafe {
            libc::free(mem::transmute(self.machine_code));
        }
    }
}

pub struct JitCompiler {
    jit_code: JitCode
}

impl JitCompiler {
    pub fn new() -> JitCompiler {
        JitCompiler {jit_code: JitCode::new()}
    }

    fn emit_move_next(&mut self) {
        self.jit_code.emit_inc_r(Reg::Rbx);
    }

    fn emit_move_prev(&mut self) {
        self.jit_code.emit_dec_r(Reg::Rbx);
    }

    fn emit_putchar(&mut self) {
        //Todo: mov rdi
        self.jit_code.emit_mov_ri(Reg::Rcx, unsafe {mem::transmute(putchar)});
        self.jit_code.emit_call_r(Reg::Rcx);
    }

    fn emit_getchar(&mut self) {
        self.jit_code.emit_mov_ri(Reg::Rcx, unsafe {mem::transmute(getchar)});
        self.jit_code.emit_call_r(Reg::Rcx);
    }

    fn emit_data_inc(&mut self) {
        self.jit_code.emit_code(&[0x48, 0x8B, 0x0C, 0x1A]); //mov rcx,QWORD PTR [rdx+rbx*1]
        self.jit_code.emit_inc_r(Reg::Rcx);
        self.jit_code.emit_code(&[0x48, 0x89, 0x0C, 0x1A]); //mov QWORD PTR [rdx+rbx*1],rcx
    }

    fn emit_data_dec(&mut self) {
        self.jit_code.emit_code(&[0x48, 0x8B, 0x0C, 0x1A]); //mov rcx,QWORD PTR [rdx+rbx*1]
        self.jit_code.emit_dec_r(Reg::Rcx);
        self.jit_code.emit_code(&[0x48, 0x89, 0x0C, 0x1A]); //mov QWORD PTR [rdx+rbx*1],rcx
    }

    pub fn compile(&mut self, code: &str) {
        self.jit_code.emit_push_r(Reg::Rbp);
        self.jit_code.emit_mov_rr(Reg::Rbp, Reg::Rsp);
        self.jit_code.emit_mov_ri(Reg::Rbx, 0); //Rbx = data_pointer
        self.jit_code.emit_mov_rr(Reg::Rdx, Reg::Rdi); //Rdx = memory_base

        let mut memory: Vec<u8>= vec![0; 10000];
        for ch in code.chars() {
            match ch {
                '>' => self.emit_move_next(),
                '<' => self.emit_move_prev(),
                '+' => self.emit_data_inc(),
                '-' => self.emit_data_dec(),
                '.' => self.emit_putchar(),
                ',' => self.emit_getchar(),
                //'[' =>,
                //']' =>,
                _ => ()
            }
        }
        self.jit_code.emit_mov_rr(Reg::Rsp, Reg::Rbp);
        self.jit_code.emit_pop_r(Reg::Rbp);
        self.jit_code.emit_ret();
        let func: fn()->i64 = unsafe { mem::transmute(self.jit_code.function()) };
        println!("{}", func());
    }
}
