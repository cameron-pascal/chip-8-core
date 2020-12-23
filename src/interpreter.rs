use crate::{callstack, opcode, timer, platform_adapter, keycodes, quirk_flags};

use callstack::*;
use opcode::*;
use timer::*;
use platform_adapter::*;
use keycodes::*;
use quirk_flags::*;

pub const RES_Y: usize = 32;
pub const RES_X: usize = 64;

const START_ADDR: usize = 0x200;
const STACK_SZ: usize = 16;
const MEM_SZ: usize = 4096;
const REG_COUNT: usize = 16;

const CHAR_TABLE_LEN: usize = 5 * 16; // 16 characters (0-F), 5 bytes each.
const CHAR_TABLE: [u8; CHAR_TABLE_LEN] = [
    0xF0, 0x90, 0x90, 0x90, 0xF0, // "0"
    0x20, 0x60, 0x20, 0x20, 0x70, // "1"
    0xF0, 0x10, 0xF0, 0x80, 0xF0, // "2"
    0xF0, 0x10, 0xF0, 0x10, 0xF0, // "3"
    0x90, 0x90, 0xF0, 0x10, 0x10, // "4"
    0xF0, 0x80, 0xF0, 0x10, 0xF0, // "5"
    0xF0, 0x80, 0xF0, 0x90, 0xF0, // "6"
    0xF0, 0x10, 0x20, 0x40, 0x40, // "7"
    0xF0, 0x90, 0xF0, 0x90, 0xF0, // "8"
    0xF0, 0x90, 0xF0, 0x10, 0xF0, // "9"
    0xF0, 0x90, 0xF0, 0x90, 0x90, // "A"
    0xE0, 0x90, 0xE0, 0x90, 0xE0, // "B"
    0xF0, 0x80, 0x80, 0x80, 0xF0, // "C"
    0xE0, 0x90, 0x90, 0x90, 0xE0, // "D"
    0xF0, 0x80, 0xF0, 0x80, 0xF0, // "E"
    0xF0, 0x80, 0xF0, 0x80, 0x80, // "F"
];

#[derive(Debug, PartialEq)]
pub enum InterpreterErr {
    CallStackEmpty,
    CallStackOverflow,
    InvalidOpcode(u16),
    InvalidRegister,
    MemFault,
    DisplayFault,
    NonMonotonicClockValue,
    RomTooLarge,
}

fn from_stack_err(stack_err: CallStackErr) -> InterpreterErr { 
    match stack_err {
        CallStackErr::StackOverflow => InterpreterErr::CallStackOverflow,
        CallStackErr::StackEmpty =>  InterpreterErr::CallStackEmpty
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct KeyAwaitOp {
    pub dest_v_reg: u8,
}

pub struct Chip8Interpreter<T>
where
    T: PlatformAdapter,
{
    pub quirks: QuirkFlags,
    pub key_press: Option<KeyCodes>,
    pub display_buffer: [[u8; RES_X]; RES_Y],
    pub memory: [u8; MEM_SZ],
    pub pc: u16,
    pub v_regs: [u8; REG_COUNT],
    pub i_reg: u16,
    pub stack: CallStack,
    pub key_await_dest_reg: Option<KeyAwaitOp>,
    pub delay_timer: Timer,
    pub sound_timer: Timer,
    pub is_sound_playing: bool,
    platform_adapter: T,
}

impl<T> Chip8Interpreter<T>
where
    T: PlatformAdapter,
{
    pub fn new(platform_adapter: T, rom: Vec<u8>) -> Result<Self, InterpreterErr> {
        let rom_len  = rom.len();
        
        if START_ADDR + rom_len >= MEM_SZ {
            return Err(InterpreterErr::RomTooLarge)
        }

        let mut interpreter = Chip8Interpreter {
            quirks: QuirkFlags::NONE,
            key_press: Option::None,
            platform_adapter,
            memory: [0; MEM_SZ],
            display_buffer: [[0; RES_X]; RES_Y],
            pc: START_ADDR as u16,
            v_regs: [0; 16],
            i_reg: 0,
            stack: CallStack::new(STACK_SZ),
            key_await_dest_reg: Option::None,
            delay_timer: Timer::new(),
            sound_timer: Timer::new(),
            is_sound_playing: false,
        };

        // Copy the character table into memory.
        for i in 0..CHAR_TABLE.len() {
            interpreter.memory[i] = CHAR_TABLE[i];
        }

        // Copy the ROM into memory at address 0x200.
        for i in 0..rom_len {
            interpreter.memory[START_ADDR + i] = rom[i];
        }

        Ok(interpreter)
    }

    pub fn step(&mut self, tick_rate: u64) -> Result<DecodedInstruction, InterpreterErr> {

        self.check_sound_timer(tick_rate)?;

        // Execution should halt if FX0A was executed, which waits until a key has been pressed.
        if !self.is_awaiting_key_press()? {
            let opcode = self.fetch_next_instruction()?;

            return match self.execute_instruction(&opcode) {
                Ok(()) => Ok(opcode),
                Err(err) => Err(err)
            }
        }

        Ok(DecodedInstruction::new())
    }

    fn is_awaiting_key_press(&mut self) -> Result<bool, InterpreterErr> {
        match self.key_await_dest_reg {
            None => Ok(false),

            Some(key_await_op) => match self.key_press {
                None => Ok(true),

                Some(keycode) => {
                    self.write_v_reg(key_await_op.dest_v_reg, keycode as u8)?;
                    self.key_await_dest_reg = Option::None;
                    Ok(false)
                }
            },
        }
    }

    fn fetch_next_instruction(&mut self) -> Result<DecodedInstruction, InterpreterErr> {
        // Opcodes are 16 bits, so read two bytes.
        let hi = self.read_mem(self.pc)? as u16;
        let lo = self.read_mem(self.pc + 1)? as u16;

        self.pc += 2;

        let instr = (hi << 8) | lo;
        Ok(opcode::decode(instr, self.quirks))
    }

    fn execute_instruction(&mut self, decoded_instr: &DecodedInstruction) -> Result<(), InterpreterErr> {
        
        match decoded_instr.opcode {
            
            OpCode::OpCode00e0() => self.execute_00e0(),

            OpCode::OpCode00ee() => self.execute_00ee(),

            OpCode::OpCode1nnn(addr) => self.execute_1nnn(addr),

            OpCode::OpCode2nnn(addr) => self.execute_2nnn(addr),

            OpCode::OpCode3xnn(vx_idx, val) => self.execute_3xnn(vx_idx, val),
            
            OpCode::OpCode4xnn(vx_idx, val) => self.execute_4xnn(vx_idx, val),
            
            OpCode::OpCode5xy0(vx_idx, vy_idx) => self.execute_5xy0(vx_idx, vy_idx),
            
            OpCode::OpCode6xnn(vx_idx, val) => self.execute_6xnn(vx_idx, val),
            
            OpCode::OpCode7xnn(vx_idx, val) => self.execute_7xnn(vx_idx, val),
            
            OpCode::OpCode8xy0(vx_idx, vy_idx) => self.execute_8xy0(vx_idx, vy_idx),
            
            OpCode::OpCode8xy1(vx_idx, vy_idx) => self.execute_8xy1(vx_idx, vy_idx),
            
            OpCode::OpCode8xy2(vx_idx, vy_idx) => self.execute_8xy2(vx_idx, vy_idx),
            
            OpCode::OpCode8xy3(vx_idx, vy_idx) => self.execute_8xy3(vx_idx, vy_idx),
            
            OpCode::OpCode8xy4(vx_idx, vy_idx) => self.execute_8xy4(vx_idx, vy_idx),
            
            OpCode::OpCode8xy5(vx_idx, vy_idx) => self.execute_8xy5(vx_idx, vy_idx),
            
            OpCode::OpCode8xy6(vx_idx, vy_idx) =>  {

                if self.quirks.contains(QuirkFlags::QUIRK_8XY6) {
                    self.execute_8xy6_quirk_mode(vx_idx, vy_idx)
                } else {
                    self.execute_8xy6(vx_idx)
                }
            } 
            
            OpCode::OpCode8xy7(vx_idx, vy_idx) => self.execute_8xy7(vx_idx, vy_idx),
            
            OpCode::OpCode8xye(vx_idx, vy_idx) => {

                if self.quirks.contains(QuirkFlags::QUIRK_8XYE) {
                    self.execute_8xye_quirk_mode(vx_idx, vy_idx)
                } else {
                    self.execute_8xye(vx_idx)
                }
            } 
            
            OpCode::OpCode9xy0(vx_idx, vy_idx) => self.execute_9xy0(vx_idx, vy_idx),
            
            OpCode::OpCodeAnnn(addr) => self.execute_annn(addr),
            
            OpCode::OpCodeBnnn(addr) => self.execute_bnnn(addr),
            
            OpCode::OpCodeCxnn(vx_idx, mask) => self.execute_cxnn(vx_idx, mask),
            
            OpCode::OpCodeDxyn(vx_idx, vy_idx, count) => self.execute_dxyn(vx_idx, vy_idx, count),
            
            OpCode::OpCodeEx9e(vx_idx) => self.execute_ex9e(vx_idx),
            
            OpCode::OpCodeExa1(vx_idx) => self.execute_exa1(vx_idx),
            
            OpCode::OpCodeFx07(vx_idx) => self.execute_fx07(vx_idx),
            
            OpCode::OpCodeFx0a(vx_idx) => self.execute_fx0a(vx_idx),
            
            OpCode::OpCodeFx15(vx_idx) => self.execute_fx15(vx_idx),
            
            OpCode::OpCodeFx18(vx_idx) => self.execute_fx18(vx_idx),
            
            OpCode::OpCodeFx1e(vx_idx) => {

                if self.quirks.contains(QuirkFlags::QUIRK_FX1E) {
                    self.execute_fx1e_quirk_mode(vx_idx)
                } else {
                    self.execute_fx1e(vx_idx)
                }
            }
            
            OpCode::OpCodeFx29(vx_idx) => self.execute_fx29(vx_idx),
            
            OpCode::OpCodeFx33(vx_idx) => self.execute_fx33(vx_idx),
            
            OpCode::OpCodeFx55(vx_idx) => {

                if self.quirks.contains(QuirkFlags::QUIRK_FX55) {
                    self.execute_fx55_quirk_mode(vx_idx)
                } else {
                    self.execute_fx55(vx_idx)
                }
            }
            
            OpCode::OpCodeFx65(vx_idx) => {

                if self.quirks.contains(QuirkFlags::QUIRK_FX65) {
                    self.execute_fx65_quirk_mode(vx_idx)
                } else {
                    self.execute_fx65(vx_idx)
                }
            }
            
            OpCode::OpCodeInvalid() => Err(InterpreterErr::InvalidOpcode(decoded_instr.instr))
        }
    }

    fn read_mem(&self, addr: u16) -> Result<u8, InterpreterErr> {
        let idx = addr as usize;
        if idx >= MEM_SZ {
            return Err(InterpreterErr::MemFault);
        }

        Ok(self.memory[idx])
    }

    fn write_mem(&mut self, addr: u16, val: u8) -> Result<(), InterpreterErr> {
        let idx = addr as usize;
        if idx >= MEM_SZ {
            return Err(InterpreterErr::MemFault);
        }

        self.memory[idx] = val;
        Ok(())
    }

    fn write_v_reg(&mut self, reg_idx: u8, val: u8) -> Result<(), InterpreterErr> {
        let idx = reg_idx as usize;
        if idx >= REG_COUNT {
            return Err(InterpreterErr::InvalidRegister);
        }

        self.v_regs[idx] = val;
        Ok(())
    }

    fn read_v_reg(&self, reg_idx: u8) -> Result<u8, InterpreterErr> {
        let idx = reg_idx as usize;
        if idx >= REG_COUNT {
            return Err(InterpreterErr::InvalidRegister);
        }

        Ok(self.v_regs[idx])
    }

    fn draw(&mut self, x: u8, y: u8, val: u8) -> bool {
        let x_idx = x as usize % RES_X;
        let y_idx = y as usize % RES_Y;

        let original_val = self.display_buffer[y_idx][x_idx];
        let new_val = original_val ^ val; // The CHIP-8 sets pixels by XOR'ing the new value with the existing value.
        self.display_buffer[y_idx][x_idx] = new_val;

        if val == original_val && val == 1 {
            return true // The position has been toggled off.
        }

        false
    }

    fn start_delay_timer(&mut self, start_val: u8) {
        self.delay_timer.set(start_val);
    }

    fn start_sound_timer(&mut self, start_val: u8) {
        self.sound_timer.set(start_val);
        
        self.platform_adapter.play_sound();
        self.is_sound_playing = true;
    }

    fn check_sound_timer(&mut self, tick_rate: u64) -> Result<(), InterpreterErr> {
        let timer_val = self.sound_timer.tick(tick_rate);
        
        if timer_val == 0 && self.is_sound_playing {
            self.platform_adapter.pause_sound();
            self.is_sound_playing = false;
        }

        Ok(())
    }

    fn execute_00ee(&mut self) -> Result<(), InterpreterErr> {
        // Execute 00EE. Return from the current subroutine.
        // i.e. return;
        self.pc = self.stack.pop().map_err(from_stack_err)?;

        Ok(())
    }

    fn execute_00e0(&mut self) -> Result<(), InterpreterErr> {
        // Execute 00E0. Clear the display.
        for y in 0..RES_Y {
            for x in 0..RES_X {
                self.display_buffer[y][x] = 0;
            }
        }

        Ok(())
    }

    fn execute_1nnn(&mut self, addr: u16) -> Result<(), InterpreterErr> {
        // Execute 1NNN. Goto the address in memory at NNN.
        // i.e. goto NNN;
        self.pc = addr;

        Ok(())
    }

    fn execute_2nnn(&mut self, addr: u16) -> Result<(), InterpreterErr> {
        // Execute 2NNN. Call the subroutine in memory at NNN.
        // i.e. *(NNN)();
        self.stack.push(self.pc).map_err(from_stack_err)?;
        self.pc = addr;

        Ok(())
    }

    fn execute_3xnn(&mut self, vx_idx: u8, val: u8) -> Result<(), InterpreterErr> {
        // Read VX and skip the next instruction if VX does not equal NN.
        // i.e. if (VX != VY) { skip; }
        let vx_val = self.read_v_reg(vx_idx)?;

        if vx_val == val {
            self.pc += 2;
        }

        Ok(())
    }

    fn execute_4xnn(&mut self, vx_idx: u8, val: u8) -> Result<(), InterpreterErr> {
        // Read VX and skip the next instruction if VX does not equal NN.
        // i.e. if (VX != VY) { skip; }
        let vx_val = self.read_v_reg(vx_idx)?;

        if vx_val != val {
            self.pc += 2;
        }

        Ok(())
    }

    fn execute_5xy0(&mut self, vx_idx: u8, vy_idx: u8) -> Result<(), InterpreterErr> {
        // Excecute 5XY0. Read VX and VY and skip the next instruction if they are equal.
        // i.e. if (VX == VY) { skip; }
        let vx_val = self.read_v_reg(vx_idx)?;
        let vy_val = self.read_v_reg(vy_idx)?;

        if vx_val == vy_val {
            self.pc += 2;
        }

        Ok(())
    }

    fn execute_6xnn(&mut self, vx_idx: u8, val: u8) -> Result<(), InterpreterErr> {
        // Execute 6XNN. Load the value NN into VX.
        // i.e. VX = NN.
        self.write_v_reg(vx_idx, val)?;

        Ok(())
    }

    fn execute_7xnn(&mut self, vx_idx: u8, val: u8) -> Result<(), InterpreterErr> {
        // Execute 7XNN. Add the value NN into VX, but do not set the carry-flag at VF if the addition overflowed.
        // i.e. VX = VX + NN;
        let vx_val = self.read_v_reg(vx_idx)?;

        let result = vx_val.wrapping_add(val);
        self.write_v_reg(vx_idx, result)?;

        Ok(())
    }

    fn execute_8xy0(&mut self, vx_idx: u8, vy_idx: u8) -> Result<(), InterpreterErr> {
        // Execute 8XYO. Copy VY into VX.
        // i.e. VX = VY;
        let vy_val = self.read_v_reg(vy_idx)?;
        self.write_v_reg(vx_idx, vy_val)?;

        Ok(())
    }

    fn execute_8xy1(&mut self, vx_idx: u8, vy_idx: u8) -> Result<(), InterpreterErr> {
        // Execute 8XY1. Bitwise OR of VX and VY.
        // i.e. VX = VX | VY;
        let vx_val = self.read_v_reg(vx_idx)?;
        let vy_val = self.read_v_reg(vy_idx)?;

        let result = vx_val | vy_val;
        self.write_v_reg(vx_idx, result)?;

        Ok(())
    }

    fn execute_8xy2(&mut self, vx_idx: u8, vy_idx: u8) -> Result<(), InterpreterErr> {
        // Execute 8XY2. Bitwise AND of VX and VY.
        // i.e. VX = VX & VY;
        let vx_val = self.read_v_reg(vx_idx)?;
        let vy_val = self.read_v_reg(vy_idx)?;

        let result = vx_val & vy_val;
        self.write_v_reg(vx_idx, result)?;

        Ok(())
    }

    fn execute_8xy3(&mut self, vx_idx: u8, vy_idx: u8) -> Result<(), InterpreterErr> {
        // Execute 8XY3. Bitwise XOR of VX and VY.
        // i.e. VX = VX ^ VY;
        let vx_val = self.read_v_reg(vx_idx)?;
        let vy_val = self.read_v_reg(vy_idx)?;

        let result = vx_val ^ vy_val;
        self.write_v_reg(vx_idx, result)?;

        Ok(())
    }

    fn execute_8xy4(&mut self, vx_idx: u8, vy_idx: u8) -> Result<(), InterpreterErr> {
        // Execute 8XY4. Add VY into VX, and set the carry-flag at VF to 0x01, if the addition overflowed; otherwise 0x00.
        // i.e. VX = VX + VY;
        let vx_val = self.read_v_reg(vx_idx)?;
        let vy_val = self.read_v_reg(vy_idx)?;

        let (result, did_overflow) = vx_val.overflowing_add(vy_val);
        self.write_v_reg(vx_idx, result)?;

        if !did_overflow {
            self.write_v_reg(0x0F, 0x00)?;
        } else {
            self.write_v_reg(0x0F, 0x01)?;
        }

        Ok(())
    }

    fn execute_8xy5(&mut self, vx_idx: u8, vy_idx: u8) -> Result<(), InterpreterErr> {
        // Execute 8XY5. Subtracts VY into VX, and set the borrow-flag at VF to 0x00 if the subtraction overflowed; otherwise 0x01.
        // i.e. VX = VX - VY;
        let vx_val = self.read_v_reg(vx_idx)?;
        let vy_val = self.read_v_reg(vy_idx)?;

        let (result, did_overflow) = vx_val.overflowing_sub(vy_val);
        self.write_v_reg(vx_idx, result)?;

        if did_overflow {
            self.write_v_reg(0x0F, 0x00)?;
        } else {
            self.write_v_reg(0x0F, 0x01)?;
        }

        Ok(())
    }

    fn execute_8xy6(&mut self, vx_idx: u8) -> Result<(), InterpreterErr> {
        // Load VF with the LSB of VX and right-shift VX by one.
        // i.e. VX = VX >> 1;
        let val = self.read_v_reg(vx_idx)?;

        let lsb = 0x01 & val;
        self.write_v_reg(0x0F, lsb)?;

        let result = val >> 1;
        self.write_v_reg(vx_idx, result)?;

        Ok(())
    }

    fn execute_8xy6_quirk_mode(&mut self, vx_idx: u8, vy_idx: u8) -> Result<(), InterpreterErr> {
        // Load VF with the LSB of VY and right-shift VY by one and store in VX.
        // i.e. VX = VY >> 1;
        let val = self.read_v_reg(vy_idx)?;

        let lsb = 0x01 & val;
        self.write_v_reg(0x0F, lsb)?;

        let result = val >> 1;
        self.write_v_reg(vx_idx, result)?;

        Ok(())
    }

    fn execute_8xy7(&mut self, vx_idx: u8, vy_idx: u8) -> Result<(), InterpreterErr> {
        // Execute 8XY7. Load VX with the result of VY - VX, and set the borrow-flag at VF to 0x00 if the subtraction overflowed; otherwise 0x01.
        // i.e. VX = VY - VX;
        let vx_val = self.read_v_reg(vx_idx)?;
        let vy_val = self.read_v_reg(vy_idx)?;

        let (result, did_overflow) = vy_val.overflowing_sub(vx_val);

        if did_overflow {
            self.write_v_reg(0x0F, 0x00)?;
        } else {
            self.write_v_reg(0x0F, 0x01)?;
        }

        self.write_v_reg(vx_idx, result)?;

        Ok(())
    }

    fn execute_8xye(&mut self, vx_idx: u8) -> Result<(), InterpreterErr> {
        // Load VF with the MSB of VX and left-shift VX by one.
        // i.e. VX = VX << 1;
        let val = self.read_v_reg(vx_idx)?;

        let msb = val >> 7;
        self.write_v_reg(0x0F, msb)?;

        let result = val << 1;
        self.write_v_reg(vx_idx, result)?;

        Ok(())
    }

    fn execute_8xye_quirk_mode(&mut self, vx_idx: u8, vy_idx: u8) -> Result<(), InterpreterErr> {
        // Load VF with the MSB of VY and left-shift VY by one and store in VX.
        // i.e. VX = VY << 1;
        let val = self.read_v_reg(vy_idx)?;

        let msb = val >> 7;
        self.write_v_reg(0x0F, msb)?;

        let result = val << 1;
        self.write_v_reg(vx_idx, result)?;

        Ok(())
    }

    fn execute_9xy0(&mut self, vx_idx: u8, vy_idx: u8) -> Result<(), InterpreterErr> {
        // Execute 9XY0. Skip the next instruction if VX does not equal VY.
        // i.e. if (VX != VY) { skip; }
        let vx_val = self.read_v_reg(vx_idx)?;
        let vy_val = self.read_v_reg(vy_idx)?;

        if vx_val != vy_val {
            self.pc += 2;
        }

        Ok(())
    }

    fn execute_annn(&mut self, addr: u16) -> Result<(), InterpreterErr> {
        // Execute ANNN. Set I to NNN.
        // i.e. I = NNN
        self.i_reg = addr;

        Ok(())
    }

    fn execute_bnnn(&mut self, addr: u16) -> Result<(), InterpreterErr> {
        // Execute BNNN. Set I to NNN + V0.
        // i.e. I = V0 + NNN
        let v0_val = self.read_v_reg(0x00)? as u16;
        self.i_reg = addr + v0_val;

        Ok(())
    }

    fn execute_cxnn(&mut self, vx_idx: u8, mask: u8) -> Result<(), InterpreterErr> {
        // Execute CXNN. Set VX to a random number masked by NN.
        // i.e VX = rand() & NN
        let rand_val = self.platform_adapter.get_random_val();

        let result = rand_val & mask;
        self.write_v_reg(vx_idx, result)?;

        Ok(())
    }

    fn execute_dxyn(&mut self, vx_idx: u8, vy_idx: u8, count: u8) -> Result<(), InterpreterErr> {
        // Execute DXYN.
        // Draw sprite with dimensions 8 x (N+1) pixels starting at address I at location (x, y).
        // XOR sprite data with display data and set VF to 1 if any pixels were toggled off.
        // i.e. draw(VX, VY, I, N);
        let x_start = self.read_v_reg(vx_idx)?;
        let y_start = self.read_v_reg(vy_idx)?;

        let addr = self.i_reg;
        let mut did_toggle_pixel_off = false;

        for line_num in 0..count {
            let sprite_data = self.read_mem(addr + line_num as u16)?;

            did_toggle_pixel_off |=
                self.draw(x_start, y_start + line_num, (sprite_data >> 7) & 1);
            did_toggle_pixel_off |=
                self.draw(x_start + 1, y_start + line_num, (sprite_data >> 6) & 1);
            did_toggle_pixel_off |=
                self.draw(x_start + 2, y_start + line_num, (sprite_data >> 5) & 1);
            did_toggle_pixel_off |=
                self.draw(x_start + 3, y_start + line_num, (sprite_data >> 4) & 1);
            did_toggle_pixel_off |=
                self.draw(x_start + 4, y_start + line_num, (sprite_data >> 3) & 1);
            did_toggle_pixel_off |=
                self.draw(x_start + 5, y_start + line_num, (sprite_data >> 2) & 1);
            did_toggle_pixel_off |=
                self.draw(x_start + 6, y_start + line_num, (sprite_data >> 1) & 1);
            did_toggle_pixel_off |=
                self.draw(x_start + 7, y_start + line_num, sprite_data & 1);
        }

        if did_toggle_pixel_off {
            self.write_v_reg(0x0F, 0x01)?;
        } else {
            self.write_v_reg(0x0F, 0x00)?;
        }

        Ok(())
    }

    fn execute_ex9e(&mut self, vx_idx: u8) -> Result<(), InterpreterErr> {
        // Execute EX9E. Skip the next instruction if VX equals the current key being pressed.
        // i.e. if (VX == get_key_press()) { skip; }
        let vx_val = self.read_v_reg(vx_idx)?;

        match self.key_press {
            
            Some(keycode) => {
                if keycode as u8 == vx_val {
                    self.pc += 2;
                }
            }

            None => {},
        };

        Ok(())
    }

    fn execute_exa1(&mut self, vx_idx: u8) -> Result<(), InterpreterErr> {
        // Execute EXA1. Skip the next instruction if VX does not equal the current key being pressed.
        // i.e. if (VX != get_key_press()) { skip; }
        let vx_val = self.read_v_reg(vx_idx)?;

        match self.key_press {
            
            Some(keycode) => {
                if keycode as u8 != vx_val {
                    self.pc += 2;
                }
            }

            None => {}
        };

        Ok(())
    }

    fn execute_fx07(&mut self, vx_idx: u8) -> Result<(), InterpreterErr> {
        // Execute FX07. Set VX to the value of the delay timer.
        // i.e VX = get_delay_value();
        let delay_value = self.delay_timer.tick(100);

        self.write_v_reg(vx_idx, delay_value)?;

        Ok(())
    }

    fn execute_fx0a(&mut self, vx_idx: u8) -> Result<(), InterpreterErr> {
        // Execute FX07. Halt execution until a key is pressed. Store the key-press in VX.
        // i.e. VX = await get_key_press();
        self.key_await_dest_reg = Option::Some(KeyAwaitOp { dest_v_reg: vx_idx });

        Ok(())
    }

    fn execute_fx15(&mut self, vx_idx: u8) -> Result<(), InterpreterErr> {
        // Execute FX15. Set the delay timer to VX.
        // i.e. delay_timer = VX;
        let val = self.read_v_reg(vx_idx)?;
        self.start_delay_timer(val);

        Ok(())
    }

    fn execute_fx18(&mut self, vx_idx: u8) -> Result<(), InterpreterErr> {
        // Execute FX18. Set the sound timer to VX and begin playing sound.
        // i.e. sound_timer = VX;
        let val = self.read_v_reg(vx_idx)?;
        self.start_sound_timer(val);

        Ok(())
    }

    fn execute_fx1e(&mut self, vx_idx: u8) -> Result<(), InterpreterErr> {
        // Execute FX1E. Add VX to I. Do not modify VF.
        // i.e. I += VX;
        let val = self.read_v_reg(vx_idx)?;
        let (sum, _) = self.i_reg.overflowing_add(val as u16);
        self.i_reg = sum;

        Ok(())
    }

    fn execute_fx1e_quirk_mode(&mut self, vx_idx: u8) -> Result<(), InterpreterErr> {
        // Execute FX1E as implemented by the Amiga CHIP-8 interpreter.
        // Add VX to I and set VF to 1 if there's a overflow; otherwise 0.
        // i.e. I += VX;
        let val = self.read_v_reg(vx_idx)?;
        
        let (sum, did_overflow) = self.i_reg.overflowing_add(val as u16);
        self.i_reg = sum;

        if did_overflow {
            self.write_v_reg(0x0F, 0x01)?;
        } else {
            self.write_v_reg(0x0F, 0x00)?;
        }

        Ok(())
    }

    fn execute_fx29(&mut self, vx_idx: u8) -> Result<(), InterpreterErr> {
        // Execute FX29. Set I to starting address of character VX.
        // i.e. I = get_char_addr(VX);
        let val = self.read_v_reg(vx_idx)?;

        self.i_reg = (val * 5) as u16; // Characters are 5 bytes long and are stored in sequential order (0-F) starting at address 0x000.
        
        Ok(())
    }

    fn execute_fx33(&mut self, vx_idx: u8) -> Result<(), InterpreterErr> {
        // Execute FX33. Store the BCD (binary coded decimal) representation of VX (including leading zeroes) starting at address I.
        // i.e. store_hundreds_place(VX, I); store_tens_place(VX, I+1); store_ones_place(VX, I+2)
        let val = self.read_v_reg(vx_idx)?;

        let hundreds_place = val / 100;
        let tens_place = (val / 10) % 10;
        let ones_place = val % 10;

        self.write_mem(self.i_reg, hundreds_place)?;
        self.write_mem(self.i_reg + 1, tens_place)?;
        self.write_mem(self.i_reg + 2, ones_place)?;
        
        Ok(())
    }

    fn execute_fx55(&mut self, vx_idx: u8) -> Result<(), InterpreterErr> {
        // Execute FX55. Dump the contents of V0-VX into memory starting at address I and do not modify I.
        // i.e for x in [0,X] { mem[I + x] = Vx; }
        for x in 0x0..=vx_idx {
            let v_reg_val = self.read_v_reg(x)?;
            self.write_mem(self.i_reg + x as u16, v_reg_val)?
        }

        Ok(())
    }

    fn execute_fx55_quirk_mode(&mut self, vx_idx: u8) -> Result<(), InterpreterErr> {
        // Execute FX55. Dump the contents of V0-VX into memory starting at address I and set I to I + X + 1.
        // i.e for x in [0,X] { mem[I + x] = Vx; } I += X + 1;
        for x in 0x0..=vx_idx {
            let v_reg_val = self.read_v_reg(x)?;
            self.write_mem(self.i_reg + x as u16, v_reg_val)?
        }

        self.i_reg += vx_idx as u16 + 1;

        Ok(())
    }

    fn execute_fx65(&mut self, vx_idx: u8) -> Result<(), InterpreterErr> {
        // Execute FX55. Load I..I+X into V0..VX and do not modify I.
        // i.e for x in [0,X] { Vx = I + x; }
        for x in 0x0..=vx_idx {
            let mem_val = self.read_mem(self.i_reg + x as u16)?;
            self.write_v_reg(x, mem_val)?
        }

        Ok(())
    }

    fn execute_fx65_quirk_mode(&mut self, vx_idx: u8) -> Result<(), InterpreterErr> {
        // Execute FX55. Load I..I+X into V0..VX and set I to I + X + 1.
        // i.e for x in [0,X] { Vx = I + x; } I += X + 1;
        for x in 0x0..=vx_idx {
            let mem_val = self.read_mem(self.i_reg + x as u16)?;
            self.write_v_reg(x, mem_val)?
        }

        self.i_reg += vx_idx as u16 + 1;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockPlatform {
        random_val: u8,
        play_count: u8,
        pause_count: u8,
    }

    impl MockPlatform {
        fn new() -> Self {
            MockPlatform {
                random_val: 0,
                play_count: 0,
                pause_count: 0,
            }
        }
    }

    impl PlatformAdapter for MockPlatform {
        fn play_sound(&mut self) {
            self.play_count += 1;
        }

        fn pause_sound(&mut self) {
            self.pause_count += 1;
        }

        fn get_random_val(&self) -> u8 {
            self.random_val
        }
    }

    fn get_new_interpreter() -> Chip8Interpreter<MockPlatform> {
        Chip8Interpreter::new(MockPlatform::new(), Vec::new()).unwrap()
    }

    #[test]
    fn fetch_instruction() {
        let mut interpreter = get_new_interpreter();
        let start_addr = START_ADDR as u16;
        interpreter.write_mem(start_addr, 0xD1).unwrap();
        interpreter.write_mem(start_addr + 1, 0xCD).unwrap();

        let decoded_instr = interpreter.fetch_next_instruction().unwrap();
        assert_eq!(start_addr + 2, interpreter.pc);

        assert_eq!(OpCode::OpCodeDxyn(0x1, 0xC, 0xD), decoded_instr.opcode);
    }

    #[test]
    fn execute_00e0_test() {
        // Tests instruction OOEO, which we expect to clear the display.
        let mut interpreter = get_new_interpreter();
        for y in 0..RES_Y {
            for x in 0..RES_X {
                interpreter.display_buffer[y][x] = 0;
            }
        }

        interpreter.execute_instruction(&opcode::decode(0x00E0, QuirkFlags::NONE)).unwrap();

        for y in 0..RES_Y {
            for x in 0..RES_X {
                assert_ne!(1, interpreter.display_buffer[y][x]);
            }
        }
    }

    #[test]
    fn execute_1nnn_test() {
        // Tests 1NNN, which we expect to set the program-counter to NNN.
        let mut interpreter = get_new_interpreter();

        interpreter.execute_instruction(&opcode::decode(0x1234, QuirkFlags::NONE)).unwrap();
        assert_eq!(interpreter.pc, 0x234);
    }

    #[test]
    fn execute_2nnn_and_00ee_test() {
        // Tests 2NNN and OOEE, which respectively call a subroutine and return from a subroutine.
        // We expect the program-counter to be set to NNN by 2NNN, and then returned to its original value by 00EE.
        let mut interpreter = get_new_interpreter();
        let original_pc_val = interpreter.pc;

        // First check 2NNN (call subroutine). Program-counter should be set to 0x345.
        interpreter.execute_instruction(&opcode::decode(0x2345, QuirkFlags::NONE)).unwrap();
        assert_eq!(interpreter.pc, 0x345);

        // Then check 00EE (return from subroutine). Program-counter should be set the value it was before the call.
        interpreter.execute_instruction(&opcode::decode(0x00EE, QuirkFlags::NONE)).unwrap();

        assert_eq!(interpreter.pc, original_pc_val);
    }

    #[test]
    fn execute_3xnn_test() {
        // Tests 3XNN, which we expect to increment the program-counter if VX == NN.
        let mut interpreter = get_new_interpreter();

        // First check equals case. Program-counter should be incremented.
        let original_pc_val = interpreter.pc;
        interpreter.write_v_reg(0x4, 0x56).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x3456, QuirkFlags::NONE)).unwrap();

        assert_eq!(original_pc_val + 2, interpreter.pc);

        // Then check not-equals case.  Program-counter should not be incremented.
        let original_pc_val = interpreter.pc;
        interpreter.write_v_reg(0x4, 0x55).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x3456, QuirkFlags::NONE)).unwrap();
        assert_eq!(original_pc_val, interpreter.pc);
    }

    #[test]
    fn execute_4xnn_test() {
        // Tests 4XNN, which we expect to increment the program-counter if VX != NN.
        let mut interpreter = get_new_interpreter();

        // First check equals case. Program-counter should be incremented.
        let original_pc_val = interpreter.pc;
        interpreter.write_v_reg(0x4, 0x56).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x4456, QuirkFlags::NONE)).unwrap();
        
        assert_eq!(original_pc_val, interpreter.pc);

        // Then check not-equals case.  Program-counter should not be incremented.
        let original_pc_val = interpreter.pc;
        interpreter.write_v_reg(0x4, 0x55).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x4456, QuirkFlags::NONE)).unwrap();
        assert_eq!(original_pc_val + 2, interpreter.pc);
    }

    #[test]
    fn execute_5xy0_test() {
        // Tests 5XY0, which we expect to increment the program-counter if VX == VY.
        let mut interpreter = get_new_interpreter();

        // First check equals case. Program-counter should be incremented.
        let original_pc_val = interpreter.pc;
        interpreter.write_v_reg(0x1, 0x50).unwrap();
        interpreter.write_v_reg(0x2, 0x50).unwrap();
        
        interpreter.execute_instruction(&opcode::decode(0x5120, QuirkFlags::NONE)).unwrap();
        
        assert_eq!(original_pc_val + 2, interpreter.pc);

        // Then check not-equals case. Program-counter should not be incremented.
        let original_pc_val = interpreter.pc;
        interpreter.write_v_reg(0x1, 0x51).unwrap();
        interpreter.write_v_reg(0x2, 0x50).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x5120, QuirkFlags::NONE)).unwrap();
        
        assert_eq!(original_pc_val, interpreter.pc);
    }

    #[test]
    fn execute_6xnn_test() {
        // Tests 6XNN, which we expect to load the value NN into VX.
        let mut interpreter = get_new_interpreter();

        interpreter.execute_instruction(&opcode::decode(0x6250, QuirkFlags::NONE)).unwrap();

        let vx_val = interpreter.read_v_reg(0x02).unwrap();
        assert_eq!(0x50, vx_val);
    }

    #[test]
    fn execute_7xnn_test() {
        // Tests 7XNN, which we expect to add the value NN into VX and not change the carry-flag at VF.
        let mut interpreter = get_new_interpreter();

        // First check non-overflowed addition. VF should not be changed and the value in VX should be 0xFE + 0x01 = 0xFF.
        interpreter.write_v_reg(0x02, 0xFE).unwrap();
        interpreter.write_v_reg(0x0F, 0x0E).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x7201, QuirkFlags::NONE)).unwrap();

        let vx_val = interpreter.read_v_reg(0x02).unwrap();
        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        assert_eq!(0xFF, vx_val);
        assert_eq!(0x0E, vf_val);

        // Then check overflowed addition. VF should not be changed the value in VX should be the wrapped value of 0xFF + 0x03 = 0x02.
        interpreter.write_v_reg(0x02, 0xFF).unwrap();
        interpreter.write_v_reg(0x0F, 0x0E).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x7203, QuirkFlags::NONE)).unwrap();

        let vx_val = interpreter.read_v_reg(0x02).unwrap();
        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        assert_eq!(0x02, vx_val);
        assert_eq!(0x0E, vf_val);
    }

    #[test]
    fn execute_8xy0_test() {
        // Tests 8XY0, which we expect to copy VY into VX.
        let mut interpreter = get_new_interpreter();
        interpreter.write_v_reg(0x02, 0x0F).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x8120, QuirkFlags::NONE)).unwrap();

        let vx_val = interpreter.read_v_reg(0x01).unwrap();
        assert_eq!(0x0F, vx_val);
    }

    #[test]
    fn execute_8xy1_test() {
        // Tests 8XY1, which we expect to store VX | VY into VX.
        let mut interpreter = get_new_interpreter();
        interpreter.write_v_reg(0x01, 0b01).unwrap();
        interpreter.write_v_reg(0x02, 0b10).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x8121, QuirkFlags::NONE)).unwrap();

        let vx_val = interpreter.read_v_reg(0x01).unwrap();
        assert_eq!(0b11, vx_val);
    }

    #[test]
    fn execute_8xy2_test() {
        // Tests 8XY2, which we expect to store VX & VY into VX.
        let mut interpreter = get_new_interpreter();
        interpreter.write_v_reg(0x1, 0b110).unwrap();
        interpreter.write_v_reg(0x2, 0b101).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x8122, QuirkFlags::NONE)).unwrap();

        let vx_val = interpreter.read_v_reg(0x01).unwrap();
        assert_eq!(0b100, vx_val);
    }

    #[test]
    fn execute_8xy3_test() {
        // Tests 8XY3, which we expect to store VX ^ VY into VX.
        let mut interpreter = get_new_interpreter();
        interpreter.write_v_reg(0x01, 0b11010).unwrap();
        interpreter.write_v_reg(0x02, 0b10111).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x8123, QuirkFlags::NONE)).unwrap();

        let vx_val = interpreter.read_v_reg(0x01).unwrap();
        assert_eq!(0b01101, vx_val);
    }

    #[test]
    fn execute_8xy4_test() {
        // Tests 8XY4, which we expect to add VY into VX and set the carry-flag at VF to 0x01 if addition overflowed;
        // otherwise 0x00.
        let mut interpreter = get_new_interpreter();

        // First check non-overflowed addition. VF should be set to 0x01 and the value in VX should be 0xFE + 0x01 = 0xFF.
        interpreter.write_v_reg(0x01, 0xFE).unwrap();
        interpreter.write_v_reg(0x02, 0x01).unwrap();
        interpreter.write_v_reg(0x0F, 0x0E).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x8124, QuirkFlags::NONE)).unwrap();

        let vx_val = interpreter.read_v_reg(0x01).unwrap();
        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        assert_eq!(0xFF, vx_val);
        assert_eq!(0x00, vf_val);

        // Then check overflowed addition.
        // VF should be set to 0x01 and the value in VX should be the wrapped value of 0xFF + 0x03 = 0x2
        interpreter.write_v_reg(0x01, 0xFF).unwrap();
        interpreter.write_v_reg(0x02, 0x03).unwrap();
        interpreter.write_v_reg(0x0F, 0x0E).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x8124, QuirkFlags::NONE)).unwrap();

        let vx_val = interpreter.read_v_reg(0x01).unwrap();
        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        assert_eq!(0x02, vx_val);
        assert_eq!(0x01, vf_val);
    }

    #[test]
    fn execute_8xy5_test() {
        // Tests 8XY5, which we expect to subtract VY from VX and set the borrow-flag at VF to 0x00 if the subtraction overflowed;
        // othwerwise 0x01.
        let mut interpreter = get_new_interpreter();

        // First check non-overflowed subtraction. VF should be set to 0x00 because 0x02 - 0x01 does not overflow.
        // VX should be set to VX = 0x02 - 0x01 = 0x01.
        interpreter.write_v_reg(0x01, 0x02).unwrap();
        interpreter.write_v_reg(0x02, 0x01).unwrap();
        interpreter.write_v_reg(0x0F, 0x0E).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x8125, QuirkFlags::NONE)).unwrap();

        let vx_val = interpreter.read_v_reg(0x01).unwrap();
        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        assert_eq!(0x01, vx_val);
        assert_eq!(0x01, vf_val);

        // Then check overflowed subtraction. VF should be set to 0x01 becuase 0x01 - 0x02 overflows.
        // VX should be set to VX = 0x01 - 0x02 = 0xFF.
        interpreter.write_v_reg(0x01, 0x01).unwrap();
        interpreter.write_v_reg(0x02, 0x02).unwrap();
        interpreter.write_v_reg(0x0F, 0x0E).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x8125, QuirkFlags::NONE)).unwrap();

        let vx_val = interpreter.read_v_reg(0x01).unwrap();
        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        assert_eq!(0xFF, vx_val);
        assert_eq!(0x00, vf_val);
    }

    #[test]
    fn execute_8xy6_test() {
        // Tests 8XY6, which has two behaviors depending whether or not a quirk is toggled.
        // This test test the default behavior for 8XY6 which mirrors the CHIP-48 and S-CHIP implementations.
        // VF should be set to the LSB of VX and VX should be set to VX >> 1.
        let mut interpreter = get_new_interpreter();
        interpreter.write_v_reg(0x01, 0b_0000_1101).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x8126, QuirkFlags::NONE)).unwrap();

        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        let vx_val = interpreter.read_v_reg(0x01).unwrap();
        assert_eq!(0x01, vf_val);
        assert_eq!(0b_0000_0110, vx_val);
    }

    #[test]
    fn execute_8xy6_quirk_mode_test() {
        // Tests 8XY6, which has two behaviors depending whether or not a quirk is toggled.
        // This test test the quirk-mode behavior for 8XY6 which mirrors the original CHIP-8 implementation.
        // VF should be set to the LSB of VY and VX should be set to VY >> 1.
        let mut interpreter = get_new_interpreter();
        interpreter.quirks = QuirkFlags::QUIRK_8XY6;
        interpreter.write_v_reg(0x02, 0b_0000_1101).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x8126, QuirkFlags::QUIRK_8XY6)).unwrap();

        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        let vx_val = interpreter.read_v_reg(0x01).unwrap();
        assert_eq!(0x01, vf_val);
        assert_eq!(0b_0000_0110, vx_val);
    }

    #[test]
    fn execute_8xy7_test() {
        // Tests 8XY7, which subtracts VX from VY and stores the result in VX.
        // If the subtraction overflowed the VF register should be set to 0x00; otherwise 0x01.
        let mut interpreter = get_new_interpreter();

        // First check non-overflowed subtraction. VF should be set to 0x00 because 0x02 - 0x01 does not overflow.
        // VX should be set to VX = 0x02 - 0x01 = 0x01.
        interpreter.write_v_reg(0x01, 0x01).unwrap();
        interpreter.write_v_reg(0x02, 0x02).unwrap();
        interpreter.write_v_reg(0x0F, 0x0E).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x8127, QuirkFlags::NONE)).unwrap();

        let vx_val = interpreter.read_v_reg(0x01).unwrap();
        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        assert_eq!(0x01, vx_val);
        assert_eq!(0x01, vf_val);

        // Then check overflowed subtraction. VF should be set to 0x01 becuase 0x01 - 0x02 overflows.
        // VX should be set to VX = 0x01 - 0x02 = 0xFF.
        interpreter.write_v_reg(0x01, 0x02).unwrap();
        interpreter.write_v_reg(0x02, 0x01).unwrap();
        interpreter.write_v_reg(0x0F, 0x0E).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x8127, QuirkFlags::NONE)).unwrap();

        let vx_val = interpreter.read_v_reg(0x01).unwrap();
        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        assert_eq!(0xFF, vx_val);
        assert_eq!(0x00, vf_val);
    }

    #[test]
    fn execute_8xye_test() {
        // Tests 8XYE, which has two behaviors depending whether or not a quirk is toggled.
        // This test test the default behavior for 8XYE which mirrors the CHIP-48 and S-CHIP implementations.
        // VF should be set to the MSB of VX and VX should be set to VX << 1.
        let mut interpreter = get_new_interpreter();
        interpreter.write_v_reg(0x01, 0b_1000_1111).unwrap();
        
        interpreter.execute_instruction(&opcode::decode(0x812E, QuirkFlags::NONE)).unwrap();
        
        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        let vx_val = interpreter.read_v_reg(0x01).unwrap();
        assert_eq!(0x01, vf_val);
        assert_eq!(0b_0001_1110, vx_val);
    }

    #[test]
    fn execute_8xye_quirk_mode_test() {
        // Tests 8XYE, which has two behaviors depending whether or not a quirk is toggled.
        // This test test the quirk-mode behavior for 8XYE which mirrors the original CHIP-8 implementation.
        // VF should be set to the MSB of VY and VX should be set to VY << 1.
        let mut interpreter = get_new_interpreter();
        interpreter.quirks = QuirkFlags::QUIRK_8XYE;
        interpreter.write_v_reg(0x02, 0b_1000_1111).unwrap();
        
        interpreter.execute_instruction(&opcode::decode(0x812E, QuirkFlags::QUIRK_8XYE)).unwrap();
        
        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        let vx_val = interpreter.read_v_reg(0x01).unwrap();
        assert_eq!(0x01, vf_val);
        assert_eq!(0b_0001_1110, vx_val);
    }

    #[test]
    fn execute_9xy0_test() {
        // Tests 9XY0, which we expect to increment the program-counter if VX != VY.
        let mut interpreter = get_new_interpreter();

        // First check equals case. Program-counter should not be incremented.
        let original_pc_val = interpreter.pc;
        interpreter.write_v_reg(0x1, 0x50).unwrap();
        interpreter.write_v_reg(0x2, 0x50).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x9120, QuirkFlags::NONE)).unwrap();

        assert_eq!(original_pc_val, interpreter.pc);

        // Then check not-equals case. Program-counter should be incremented.
        let original_pc_val = interpreter.pc;
        interpreter.write_v_reg(0x1, 0x51).unwrap();
        interpreter.write_v_reg(0x2, 0x50).unwrap();

        interpreter.execute_instruction(&opcode::decode(0x9120, QuirkFlags::NONE)).unwrap();

        assert_eq!(original_pc_val + 2, interpreter.pc);
    }

    #[test]
    fn execute_annn_test() {
        // Tests ANNN, which we expect to set I to NNN.
        let mut interpreter = get_new_interpreter();

        interpreter.execute_instruction(&opcode::decode(0xA023, QuirkFlags::NONE)).unwrap();

        assert_eq!(0x023, interpreter.i_reg);
    }

    #[test]
    fn execute_bnnn_test() {
        // Tests BNNN, which we expect to set I to NNN + V0.
        let mut interpreter = get_new_interpreter();
        interpreter.write_v_reg(0x00, 0x02).unwrap();

        interpreter.execute_instruction(&opcode::decode(0xB123, QuirkFlags::NONE)).unwrap();

        assert_eq!(0x125, interpreter.i_reg);
    }

    #[test]
    fn execute_cxnn_test() {
        // Tests CXNN, which we expect to set VX to a random number masked by NN.
        let mut interpreter = get_new_interpreter();
        interpreter.platform_adapter.random_val = 0b_0111_1111;

        let mask = 0b_0000_0000_1001_1001;
        let instr = 0xC100 | mask;

        interpreter.execute_instruction(&opcode::decode(instr, QuirkFlags::NONE)).unwrap();

        let vx_val = interpreter.read_v_reg(0x01).unwrap();
        assert_eq!(0b_0001_1001, vx_val);
    }

    //#[test] TODO: failing, fix assertions
    fn execute_dxyn_test() {
        // Test DXYN, which we expect to draw N+1 sprites starting at position (x,y)
        // with the sprites stored starting at address I.
        // If any sprites were drawn over existing sprites, we expect VF to be set to 0x01; otherwise 0x00.
        let mut interpreter = get_new_interpreter();
        interpreter.write_v_reg(0x0F, 0x0E).unwrap();

        // First check drawing with no overwriting. VF should 0x00 after executing.
        interpreter.i_reg = 0x01;
        interpreter.write_v_reg(0x01, 0x04).unwrap();
        interpreter.write_v_reg(0x02, 0x05).unwrap();
        interpreter.write_mem(0x01, 0b_10101010).unwrap();
        interpreter.write_mem(0x02, 0b_01010101).unwrap();
        interpreter.write_mem(0x03, 0b_00000000).unwrap();
        
        interpreter.execute_instruction(&opcode::decode(0xD122, QuirkFlags::NONE)).unwrap();

        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        assert_eq!(0x00, vf_val);

        // This checks the first sprite.
        assert_eq!(1, interpreter.display_buffer[5][4]);
        assert_eq!(0, interpreter.display_buffer[5][5]);
        assert_eq!(1, interpreter.display_buffer[5][6]);
        assert_eq!(0, interpreter.display_buffer[5][7]);
        assert_eq!(1, interpreter.display_buffer[5][8]);
        assert_eq!(0, interpreter.display_buffer[5][9]);
        assert_eq!(1, interpreter.display_buffer[5][10]);
        assert_eq!(0, interpreter.display_buffer[5][11]);

        // This checks the second sprite.
        assert_eq!(0, interpreter.display_buffer[6][4]);
        assert_eq!(1, interpreter.display_buffer[6][5]);
        assert_eq!(0, interpreter.display_buffer[6][6]);
        assert_eq!(1, interpreter.display_buffer[6][7]);
        assert_eq!(0, interpreter.display_buffer[6][8]);
        assert_eq!(1, interpreter.display_buffer[6][9]);
        assert_eq!(0, interpreter.display_buffer[6][10]);
        assert_eq!(1, interpreter.display_buffer[6][11]);

        // This checks the third sprite. (Which was unset.)
        assert_eq!(0, interpreter.display_buffer[7][4]);
        assert_eq!(0, interpreter.display_buffer[7][5]);
        assert_eq!(0, interpreter.display_buffer[7][6]);
        assert_eq!(0, interpreter.display_buffer[7][7]);
        assert_eq!(0, interpreter.display_buffer[7][8]);
        assert_eq!(0, interpreter.display_buffer[7][9]);
        assert_eq!(0, interpreter.display_buffer[7][10]);
        assert_eq!(0, interpreter.display_buffer[7][11]);

        // Second check a colliding sprite.
        interpreter.i_reg = 0x01;
        interpreter.write_v_reg(0x01, 0x0B).unwrap();
        interpreter.write_v_reg(0x02, 0x06).unwrap();
        interpreter.write_mem(0x01, 0b_11111111).unwrap();

        interpreter.execute_instruction(&opcode::decode(0xD120, QuirkFlags::NONE)).unwrap();

        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        assert_eq!(0x00, vf_val);

        assert_eq!(1, interpreter.display_buffer[6][11]);
        assert_eq!(0, interpreter.display_buffer[6][12]);
        assert_eq!(0, interpreter.display_buffer[6][13]);
        assert_eq!(0, interpreter.display_buffer[6][14]);
        assert_eq!(0, interpreter.display_buffer[6][15]);
        assert_eq!(0, interpreter.display_buffer[6][16]);
        assert_eq!(0, interpreter.display_buffer[6][17]);
        assert_eq!(0, interpreter.display_buffer[6][18]);

        // Third check wrapping.
        // TODO
    }

    #[test]
    fn execute_ex9e_test() {
        // Test EX9E. Which we expect to increment the program counter if VX equals the current key-press.
        let mut interpreter = get_new_interpreter();

        // First tests equals case.
        interpreter.key_press = Some(KeyCodes::Key2);
        interpreter.write_v_reg(0x01, 0x02).unwrap();
        let original_pc_val = interpreter.pc;

        interpreter.execute_instruction(&opcode::decode(0xE19E, QuirkFlags::NONE)).unwrap();

        assert_eq!(original_pc_val + 2, interpreter.pc);

        // Then test not-equals case.
        interpreter.key_press = Some(KeyCodes::Key3);
        interpreter.write_v_reg(0x01, 0x02).unwrap();
        let original_pc_val = interpreter.pc;
        
        interpreter.execute_instruction(&opcode::decode(0xE19E, QuirkFlags::NONE)).unwrap();

        assert_eq!(original_pc_val, interpreter.pc);
    }

    #[test]
    fn execute_exa1_test() {
        // Test EXA1. Which we expect to increment the program counter if VX does not equal the current key-press.
        let mut interpreter = get_new_interpreter();

        // First tests equals case.
        interpreter.key_press = Some(KeyCodes::Key2);
        interpreter.write_v_reg(0x01, 0x02).unwrap();
        let original_pc_val = interpreter.pc;

        interpreter.execute_instruction(&opcode::decode(0xE1A1, QuirkFlags::NONE)).unwrap();
        
        assert_eq!(original_pc_val, interpreter.pc);

        // Then test not-equals case.
        interpreter.key_press = Some(KeyCodes::Key3);
        interpreter.write_v_reg(0x01, 0x02).unwrap();
        let original_pc_val = interpreter.pc;

        interpreter.execute_instruction(&opcode::decode(0xE1A1, QuirkFlags::NONE)).unwrap();
        
        assert_eq!(original_pc_val + 2, interpreter.pc);
    }

    //#[test] TODO: failing
    fn execute_fx07_test() {
        // Tests FX07. Which we expect to set VX to the current value of the delay timer.
        let mut interpreter = get_new_interpreter();
        //interpreter.hardware_adapter.now_millis = 1000;
        interpreter.start_delay_timer(255);

        // First test 1 second of delay (60Hz countdown). We expect the timer value to be 255 - 60 = 195.
        //interpreter.hardware_adapter.now_millis = 2000;
        interpreter.execute_instruction(&opcode::decode(0xF107, QuirkFlags::NONE)).unwrap();

        let delay_val = interpreter.read_v_reg(0x01).unwrap();
        assert_eq!(195, delay_val);

        // Then test 5 sconds of delay (60Hz countdown). We expect the timer value to be max(0, 255 - 300) = 0.
        //interpreter.hardware_adapter.now_millis = 6000;
        interpreter.execute_instruction(&opcode::decode(0xF107, QuirkFlags::NONE)).unwrap();

        let delay_val = interpreter.read_v_reg(0x01).unwrap();
        assert_eq!(0, delay_val);
    }

    #[test]
    fn execute_fx0a_test() {
        // Tests FX07. Which we expect to halt execution until a key is pressed and store that key-press in VX.
        let mut interpreter = get_new_interpreter();
        let start_addr = START_ADDR as u16;

        // Put opcode 0xF10A in memory. This is what we're testing.
        interpreter.write_mem(start_addr, 0xF1).unwrap();
        interpreter.write_mem(start_addr + 1, 0x0A).unwrap();

        // Put opcode 0xF207 in memory (this could be any opcode). This is so the second time we step, there's a valid instruction.
        interpreter.write_mem(start_addr + 2, 0xF2).unwrap();
        interpreter.write_mem(start_addr + 3, 0x07).unwrap();

        // First check mnemonic for 0xF10A.
        interpreter.step(100).unwrap(); // Tick rate is irrelevant here.
        let pc_val_start = interpreter.pc;

        // Second check that program-counter did not move.
        interpreter.step(100).unwrap(); // Tick rate is irrelevant here.
        assert_eq!(pc_val_start, interpreter.pc);

        // Third check that program-counter moves after a key-press, VX was correctly set, and the key-await was cleared out.
        interpreter.key_press = Option::Some(KeyCodes::KeyA);
        interpreter.step(100).unwrap(); // This should move the program-counter by 2. Tick rate is irrelevant here.
        let vx_val = interpreter.read_v_reg(0x01).unwrap();
        assert_eq!(pc_val_start + 2, interpreter.pc);
        assert_eq!(vx_val, 0x0A);
        assert_eq!(Option::None, interpreter.key_await_dest_reg);
    }

    #[test]
    fn execute_fx15_test() {
        // Tests FX15. Which we expect to set the delay timer to VX.
        let mut interpreter = get_new_interpreter();
        interpreter.write_v_reg(0x01, 0x0B).unwrap();

        interpreter.execute_instruction(&opcode::decode(0xF115, QuirkFlags::NONE)).unwrap();
        
        assert_eq!(0x0B, interpreter.delay_timer.start_val);
    }

    #[test]
    fn execute_fx18_test() {
        // Tests FX18. Which we expect to set the sound timer to VX.
        let mut interpreter = get_new_interpreter();
        let start_addr = START_ADDR as u16;

        interpreter.write_v_reg(0x01, 0x01).unwrap();

        interpreter.execute_instruction(&opcode::decode(0xF118, QuirkFlags::NONE)).unwrap();
        
        assert_eq!(0x01, interpreter.sound_timer.start_val);
        assert_eq!(true, interpreter.is_sound_playing);
        assert_eq!(1, interpreter.platform_adapter.play_count);
        assert_eq!(0, interpreter.platform_adapter.pause_count);

        interpreter.write_mem(start_addr, 0xF2).unwrap();
        interpreter.write_mem(start_addr + 1, 0x07).unwrap();
        interpreter.step(1).unwrap();
        assert_eq!(false, interpreter.is_sound_playing);
        assert_eq!(1, interpreter.platform_adapter.play_count);
        assert_eq!(1, interpreter.platform_adapter.pause_count);
    }

    #[test]
    fn execute_fx1e_test() {
        // Tests FX1E, which has two behaviors depending whether or not a quirk is toggled.
        // This test test the default behavior for 8XY6 which mirrors the original CHIP-8 implementation.
        // VX should be added to I and VF should not be modified.
        let mut interpreter = get_new_interpreter();
        interpreter.write_v_reg(0x01, 0x05).unwrap();
        interpreter.write_v_reg(0x0F, 0x0E).unwrap();
        interpreter.i_reg = 0x05;

        interpreter.execute_instruction(&opcode::decode(0xF11E, QuirkFlags::NONE)).unwrap();

        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        assert_eq!(0x0E, vf_val);
        assert_eq!(0x0A, interpreter.i_reg);
    }

    #[test]
    fn execute_fx1e_quirk_mode_test() {
        // Tests FX1E, which has two behaviors depending whether or not a quirk is toggled.
        // This test test the quirk-mode behavior for FX1E which mirrors the Amiga implementation.
        // VX should be added to I and VF should be set to 1 if the addition overflowed; otherwise 0.

        // First test the non-overflow case. VF should be 0. I should be I = 0x05 + 0x05 = 0x0A.
        let mut interpreter = get_new_interpreter();
        interpreter.quirks = QuirkFlags::QUIRK_FX1E;
        interpreter.write_v_reg(0x01, 0x05).unwrap();
        interpreter.write_v_reg(0x0F, 0x0E).unwrap();
        interpreter.i_reg = 0x05;

        interpreter.execute_instruction(&opcode::decode(0xF11E, QuirkFlags::QUIRK_FX1E)).unwrap();

        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        assert_eq!(0x00, vf_val);
        assert_eq!(0x0A, interpreter.i_reg);

        // Then test the overflow case. VF should be 1. I should be I = 0x05 + 0xFFFF = 0x04.
        let mut interpreter = get_new_interpreter();
        interpreter.quirks = QuirkFlags::QUIRK_FX1E;
        interpreter.write_v_reg(0x01, 0x05).unwrap();
        interpreter.write_v_reg(0x0F, 0x0E).unwrap();
        interpreter.i_reg = 0xFFFF;
        
        interpreter.execute_instruction(&opcode::decode(0xF11E, QuirkFlags::QUIRK_FX1E)).unwrap();

        let vf_val = interpreter.read_v_reg(0x0F).unwrap();
        assert_eq!(0x01, vf_val);
        assert_eq!(0x04, interpreter.i_reg);
    }

    #[test]
    fn execute_fx29_test() {
        // Tests FX29, which is expected to set I to the starting address of the character stored in VX.
        let mut interpreter = get_new_interpreter();
        interpreter.write_v_reg(0x01, 0x0E).unwrap();
        interpreter.execute_instruction(&opcode::decode(0xF129, QuirkFlags::NONE)).unwrap();
        assert_eq!(0x0E * 5, interpreter.i_reg);
    }

    #[test]
    fn execute_fx33_test() {
        // Tests FX33, which we expect to store the BCD representation (including leading zeroes) of VX starting at address I.
        let mut interpreter = get_new_interpreter();

        // First test the full case.
        interpreter.i_reg = 0x222;
        interpreter.write_v_reg(0x01, 123).unwrap();
        interpreter.execute_instruction(&opcode::decode(0xF133, QuirkFlags::NONE)).unwrap();
        
        assert_eq!(1, interpreter.read_mem(interpreter.i_reg).unwrap());
        assert_eq!(2, interpreter.read_mem(interpreter.i_reg + 1).unwrap());
        assert_eq!(3, interpreter.read_mem(interpreter.i_reg + 2).unwrap());
        
        // Then test the one leading zero case.
        interpreter.i_reg = 0x222;
        interpreter.write_v_reg(0x01, 50).unwrap();
        interpreter.execute_instruction(&opcode::decode(0xF133, QuirkFlags::NONE)).unwrap();

        assert_eq!(0, interpreter.read_mem(interpreter.i_reg).unwrap());
        assert_eq!(5, interpreter.read_mem(interpreter.i_reg + 1).unwrap());
        assert_eq!(0, interpreter.read_mem(interpreter.i_reg + 2).unwrap());

        // Finally test the two leading zeroes case.
        interpreter.i_reg = 0x222;
        interpreter.write_v_reg(0x01, 9).unwrap();
        interpreter.execute_instruction(&opcode::decode(0xF133, QuirkFlags::NONE)).unwrap();

        assert_eq!(0, interpreter.read_mem(interpreter.i_reg).unwrap());
        assert_eq!(0, interpreter.read_mem(interpreter.i_reg + 1).unwrap());
        assert_eq!(9, interpreter.read_mem(interpreter.i_reg + 2).unwrap());
    }

    #[test]
    fn execute_fx55_test() {
        // Test FX55, which we expect to dump V0..VX into memory starting at I and leave I unmodified.
        let mut interpreter = get_new_interpreter();
        interpreter.i_reg = 0x234;
        let i_reg_final = interpreter.i_reg + 0xE;

        for x in 0..=0x0E {
            interpreter.write_v_reg(x, x + 1).unwrap();
        }

        interpreter.execute_instruction(&opcode::decode(0xFE55, QuirkFlags::NONE)).unwrap();
        assert_eq!(0x234, interpreter.i_reg);

        for i in 0x234..i_reg_final {
            let x = i - 0x234;
            let mem_val = interpreter.read_mem(i).unwrap();
            assert_eq!(x + 1, mem_val as u16);
        }
    }

    #[test]
    fn execute_fx55_quirk_mode_test() {
        // Test FX55, which we expect to dump V0..VX into memory starting at I and then set I to I += X + 1
        let mut interpreter = get_new_interpreter();
        interpreter.quirks = QuirkFlags::QUIRK_FX55;
        interpreter.i_reg = 0x234;
        let i_reg_final = interpreter.i_reg + 0xE;

        for x in 0..=0x0E {
            interpreter.write_v_reg(x, x + 1).unwrap();
        }

        interpreter.execute_instruction(&opcode::decode(0xFE55, QuirkFlags::QUIRK_FX55)).unwrap();
        assert_eq!(i_reg_final + 1, interpreter.i_reg);

        for i in 0x234..=i_reg_final {
            let x = i - 0x234;
            let mem_val = interpreter.read_mem(i).unwrap();
            assert_eq!(x + 1, mem_val as u16);
        }
    }

    #[test]
    fn execute_fx65_test() {
        // Tests FX65, which we exepct to load V0..VX from I to I+X and then set I and leave I unmodified.
        let mut interpreter = get_new_interpreter();
        interpreter.i_reg = 0x234;

        for x in 0..=0x0E {
            interpreter.write_mem(interpreter.i_reg + x, x as u8 + 1).unwrap();
        }

        interpreter.execute_instruction(&opcode::decode(0xFE65, QuirkFlags::NONE)).unwrap();
        assert_eq!(0x234, interpreter.i_reg);

        for x in 0..=0x0E {
            let v_reg_val = interpreter.read_v_reg(x).unwrap();
            assert_eq!(x + 1, v_reg_val);
        }
    }

    #[test]
    fn execute_fx65_quirk_mode_test() {
         // Tests FX65, which we exepct to load V0..VX from I to I+X and then set I to I += X + 1.
         let mut interpreter = get_new_interpreter();
         interpreter.quirks = QuirkFlags::QUIRK_FX65;
         interpreter.i_reg = 0x234;
         let i_reg_final = interpreter.i_reg + 0xE;
 
         for x in 0..=0x0E {
             interpreter.write_mem(interpreter.i_reg + x, x as u8 + 1).unwrap();
         }
 
         interpreter.execute_instruction(&opcode::decode(0xFE65, QuirkFlags::QUIRK_FX65)).unwrap();
         assert_eq!(i_reg_final + 1, interpreter.i_reg);
 
         for x in 0..=0x0E {
             let v_reg_val = interpreter.read_v_reg(x).unwrap();
             assert_eq!(x + 1, v_reg_val);
         }
    }
}