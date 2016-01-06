extern crate libc;

extern {
    fn memcpy(dest: *mut libc::c_void, srouce: *mut libc::c_void, n: libc::size_t) -> *mut libc::c_void;
    fn putchar(ch: libc::c_int) -> libc::c_int;
}

use std;
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

enum Jump {
    Jmp,
    Jz
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

    //In reloc_tbl the dest is absolute address
    reloc_tbl: HashMap<usize, u64>,

    //In patch_tbl the dest is offset to machine_code
    patch_tbl: HashMap<usize, usize>
}

impl JitCode {
    fn new() -> JitCode {
        let mut jc = JitCode {
                machine_code: 0 as *mut u8,
                code_size: 0,
                page_size: 0,
                buf_size: 0,
                reloc_tbl: HashMap::new(),
                patch_tbl: HashMap::new()};
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
            Reg::Rdx => self.emit_code(&[0x52]),
            Reg::Rbx => self.emit_code(&[0x53]),
            _ => JitCode::panic(PanicReason::NotSupported)
        }
    }

    fn emit_mov_rr(&mut self, op1: Reg, op2: Reg) {
        match (op1, op2) {
            (Reg::Rbp, Reg::Rsp) => self.emit_code(&[0x48, 0x89, 0xe5]),
            (Reg::Rsp, Reg::Rbp) => self.emit_code(&[0x48, 0x89, 0xEC]),
            (Reg::Rdx, Reg::Rdi) => self.emit_code(&[0x48, 0x89, 0xFA]),
            (Reg::Rdi, Reg::Rax) => self.emit_code(&[0x48, 0x89, 0xC7]),
            (Reg::Rdi, Reg::Rbx) => self.emit_code(&[0x48, 0x89, 0xDF]),
            (Reg::Rdi, Reg::Rcx) => self.emit_code(&[0x48, 0x89, 0xCF]),
            (Reg::Rdi, Reg::Rdx) => self.emit_code(&[0x48, 0x89, 0xD7]),
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
            Reg::Rbx => self.emit_code(&[0x5B]),
            Reg::Rdx => self.emit_code(&[0x5A]),
            _ => JitCode::panic(PanicReason::NotSupported)
        }
    }

    fn emit_ret(&mut self) {
        self.emit_code(&[0xC3]);
    }

    fn emit_jmp_with_patchback(&mut self, jump_type: Jump) -> usize{
        match jump_type {
            Jump::Jz => self.emit_code(&[0x0F, 0x84]),
            Jump::Jmp => self.emit_code(&[0xE9])
        }
        let patch_addr = self.code_size;
        self.emit_code(&[0x12, 0x34, 0x56, 0x78]);
        patch_addr
    }

    fn patch(&mut self, patch_addr: usize, dest: usize) {
        self.patch_tbl.insert(patch_addr, dest);
    }

    fn fill_offset(&self, addr: usize, offset: i64) {
        if !JitCode::imm_is_i32(offset) {
            panic!("Too far to call a subroutine");
        }

        let le_offset = (offset as i32).to_le();
        let code = JitCode::get_raw_slice(&le_offset);
        unsafe {
            for i in 0..code.len() {
                *self.machine_code.offset((addr + i) as isize) = code[i];
            }
        }
    }

    fn reloc(&mut self) {
        for (addr, dest) in &self.reloc_tbl {
            let code_addr = self.machine_code as u64 + *addr as u64;
            let offset = (*dest as i64) - (code_addr as i64) - 4;
            self.fill_offset(*addr, offset);
        }
    }

    fn patch_back(&mut self) {
        for (addr, dest) in &self.patch_tbl {
            let code_addr = self.machine_code as u64 + *addr as u64;
            let dest_addr = self.machine_code as u64 + *dest as u64;
            let offset = (dest_addr as i64) - (code_addr as i64) - 4;
            self.fill_offset(*addr, offset);
        }
    }

    fn function(&mut self) -> *const u8 {
        self.patch_back();
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

extern "C" fn test(t: i64) {
    println!("0x{:X}", t);
}

extern "C" fn _getchar() -> i32 {
    let mut tmp_str = String::new();
    std::io::stdin().read_line(&mut tmp_str).unwrap();
    tmp_str.chars().next().unwrap() as i32
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

    fn emit_push_regs(&mut self) {
        self.jit_code.emit_push_r(Reg::Rbx);
        self.jit_code.emit_push_r(Reg::Rdx);
    }

    fn emit_pop_regs(&mut self) {
        self.jit_code.emit_pop_r(Reg::Rdx);
        self.jit_code.emit_pop_r(Reg::Rbx);
    }

    fn emit_putchar(&mut self) {
        self.emit_push_regs();

        self.jit_code.emit_mov_ri(Reg::Rcx, 0);
        self.jit_code.emit_code(&[0x8A, 0x0C, 0x1A]); //mov cl, BYTE PTR [rdx+rbx*1]
        self.jit_code.emit_mov_rr(Reg::Rdi, Reg::Rcx);
        self.jit_code.emit_mov_ri(Reg::Rcx, unsafe {mem::transmute(putchar)});
        self.jit_code.emit_call_r(Reg::Rcx);
        self.jit_code.emit_mov_ri(Reg::Rcx, unsafe {mem::transmute(libc::fflush)});
        self.jit_code.emit_call_r(Reg::Rcx);

        self.emit_pop_regs();
    }

    fn debug_print_reg(&mut self, reg: Reg) {
        self.emit_push_regs();
        self.jit_code.emit_mov_rr(Reg::Rdi, reg);
        self.jit_code.emit_mov_ri(Reg::Rcx, unsafe {mem::transmute(test)});
        self.jit_code.emit_call_r(Reg::Rcx);
        self.emit_pop_regs();
    }

    fn emit_getchar(&mut self) {
        self.emit_push_regs();

        self.jit_code.emit_mov_ri(Reg::Rcx, unsafe {mem::transmute(_getchar)});
        self.jit_code.emit_call_r(Reg::Rcx);
        self.emit_pop_regs();

        self.jit_code.emit_code(&[0x88, 0x04, 0x1a]); //mov BYTE PTR [rdx+rbx*1], al
    }

    fn emit_data_inc(&mut self) {
        self.jit_code.emit_code(&[0x8A, 0x0C, 0x1A]); //mov cl, BYTE PTR [rdx+rbx*1]
        self.jit_code.emit_inc_r(Reg::Rcx);
        self.jit_code.emit_code(&[0x88, 0x0C, 0x1A]); //mov BYTE PTR [rdx+rbx*1], cl
    }

    fn emit_data_dec(&mut self) {
        self.jit_code.emit_code(&[0x8A, 0x0C, 0x1A]); //mov cl, BYTE PTR [rdx+rbx*1]
        self.jit_code.emit_dec_r(Reg::Rcx);
        self.jit_code.emit_code(&[0x88, 0x0C, 0x1A]); //mov BYTE PTR [rdx+rbx*1], cl
    }

    pub fn compile_and_run(&mut self, code: &str) {
        self.jit_code.emit_push_r(Reg::Rbp);
        self.jit_code.emit_mov_rr(Reg::Rbp, Reg::Rsp);
        self.emit_push_regs();
        self.jit_code.emit_mov_ri(Reg::Rbx, 0); //Rbx = data_pointer
        self.jit_code.emit_mov_rr(Reg::Rdx, Reg::Rdi); //Rdx = memory_base

        let mut memory: Vec<u8>= vec![0; 10000];
        let mut stack = Vec::new();
        for ch in code.chars() {
            match ch {
                '>' => self.emit_move_next(),
                '<' => self.emit_move_prev(),
                '+' => self.emit_data_inc(),
                '-' => self.emit_data_dec(),
                '.' => self.emit_putchar(),
                ',' => self.emit_getchar(),
                '[' => {
                    let pc = self.jit_code.code_size;
                    self.jit_code.emit_mov_ri(Reg::Rcx, 0);
                    self.jit_code.emit_code(&[0x8A, 0x0C, 0x1A]); //mov cl, BYTE PTR [rdx+rbx*1]
                    self.jit_code.emit_code(&[0x84, 0xC9]); //test cl, cl
                    let patch = self.jit_code.emit_jmp_with_patchback(Jump::Jz);
                    stack.push((pc, patch));
                    self.jit_code.emit_dec_r(Reg::Rcx);
                },
                ']' => {
                    match stack.pop() {
                        Some((pc, patch)) => {
                            let p = self.jit_code.emit_jmp_with_patchback(Jump::Jmp);
                            self.jit_code.patch(p, pc);
                            let dest = self.jit_code.code_size;
                            self.jit_code.patch(patch, dest);
                        },
                        _ => panic!("Unmatched brackets")
                    }

                },
                _ => ()
            }
        }
        self.emit_pop_regs();
        self.jit_code.emit_mov_rr(Reg::Rsp, Reg::Rbp);
        self.jit_code.emit_pop_r(Reg::Rbp);
        self.jit_code.emit_ret();
        let func: extern "C" fn(*mut u8) = unsafe { mem::transmute(self.jit_code.function()) };
        func(memory.as_mut_ptr());
    }
}
