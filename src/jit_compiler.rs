extern crate libc;

extern {
    fn memcpy(dest: *mut libc::c_void, srouce: *mut libc::c_void, n: libc::size_t) -> *mut libc::c_void;
}

use std::mem;

const DEFAULT_NUM_PAGES: usize = 2;
struct JitCode {
    machine_code: *mut u8,
    code_size: usize,
    buf_size: usize,
    page_size: usize
}

impl JitCode {
    fn new() -> JitCode {
        let mut jc = JitCode {machine_code: 0 as *mut u8, code_size: 0, page_size: 0, buf_size: 0};
        JitCode::expand_buffer(&mut jc);
        jc
    }

    fn expand_buffer(this: &mut JitCode) {
        unsafe{
            let mut memory: *mut libc::c_void = 0 as *mut libc::c_void;

            let page_size = if this.page_size == 0 {
                    libc::sysconf(libc::_SC_PAGESIZE) as usize
                    //16
                } else {
                    this.page_size
                };

            let buf_size = if this.buf_size == 0 {
                    DEFAULT_NUM_PAGES * page_size
                } else {
                    this.buf_size * 2
                };

            match libc::posix_memalign(&mut memory, page_size, buf_size) {
                0 => (),
                _ => panic!("Error while allocating memory")
            }

            libc::mprotect(memory, buf_size, libc::PROT_WRITE | libc::PROT_READ);

            if !this.machine_code.is_null() {
                memcpy(memory, mem::transmute(this.machine_code), this.buf_size);
                libc::free(mem::transmute(this.machine_code));
            }

            this.machine_code = mem::transmute(memory);
            this.buf_size = buf_size;
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

    fn emit_push_r(&mut self, op: &str) {
        match op.to_lowercase().as_ref() {
            "rbp" => self.emit_code(&[0x55]),
            _ => panic!("Not supported assembly")
        }
    }

    fn emit_mov_rr(&mut self, op1: &str, op2: &str) {
        match (op1.to_lowercase().as_ref(), op2.to_lowercase().as_ref()) {
            ("rbp", "rsp") => self.emit_code(&[0x48, 0x89, 0xe5]),
            ("rsp", "rbp") => self.emit_code(&[0x48, 0x89, 0xEC]),
            _ => panic!("Not supported assembly")
        }
    }

    fn emit_mov_ri(&mut self, op1: &str/*, op2: &str*/) {
        match op1.to_lowercase().as_ref() {
            "rax" => self.emit_code(&[0x48, 0xC7, 0xC0, 0x09, 0x00, 0x00, 0x00]),
            _ => panic!("Not supported assembly")
        }
    }

    fn emit_pop_r(&mut self, op: &str) {
        match op.to_lowercase().as_ref() {
            "rbp" => self.emit_code(&[0x5D]),
            _ => panic!("Not supported assembly")
        }
    }

    fn emit_ret(&mut self) {
        self.emit_code(&[0xC3]);
    }

    fn function(&mut self) -> (fn()->i64) {
        unsafe {
            libc::mprotect(mem::transmute(self.machine_code), self.buf_size, libc::PROT_READ | libc::PROT_EXEC);
            mem::transmute(self.machine_code)
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

    pub fn compile(&mut self, code: &str) {
        self.jit_code.emit_push_r("rbp");
        self.jit_code.emit_mov_rr("rbp", "rsp");
        self.jit_code.emit_mov_ri("rax");
        self.jit_code.emit_pop_r("rbp");
        self.jit_code.emit_ret();
        let func = self.jit_code.function();
        println!("{}", func());
    }
}
