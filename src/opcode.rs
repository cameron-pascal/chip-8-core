use crate::quirk_flags::QuirkFlags;

#[derive(Debug, PartialEq)]
pub struct DecodedInstruction {
    pub instr: u16,
    pub opcode: OpCode,
    pub mnemonic: String
}

impl DecodedInstruction {

    pub fn new() -> Self {
        DecodedInstruction { instr: 0, opcode: OpCode::OpCodeInvalid(), mnemonic: "".to_string() }
    }
}

#[derive(Debug, PartialEq)]
pub enum OpCode {
    // Mnenomic notation based on "Cowgod's Chip-8 Technical Reference v1.0"
    // http://devernay.free.fr/hacks/chip8/C8TECH10.HTM

    OpCode00e0(),            // CLS
    OpCode00ee(),            // RET
    OpCode1nnn(u16),         // JP   addr
    OpCode2nnn(u16),         // CALL addr
    OpCode3xnn(u8, u8),      // SE   Vx,  byte
    OpCode4xnn(u8, u8),      // SNE  Vx,  byte
    OpCode5xy0(u8, u8),      // SE   Vx,  Vy
    OpCode6xnn(u8, u8),      // LD   Vx,  byte
    OpCode7xnn(u8, u8),      // ADD  Vx,  byte
    OpCode8xy0(u8, u8),      // LD   Vx,  Vy
    OpCode8xy1(u8, u8),      // OR   Vx,  Vy
    OpCode8xy2(u8, u8),      // AND  Vx,  Vy
    OpCode8xy3(u8, u8),      // XOR  Vx,  Vy
    OpCode8xy4(u8, u8),      // ADD  Vx,  Vy
    OpCode8xy5(u8, u8),      // SUB  Vx,  Vy
    OpCode8xy6(u8, u8),      // SHR  Vx   {, Vy}     ; quirked
    OpCode8xy7(u8, u8),      // SUBN Vx,  Vy
    OpCode8xye(u8, u8),      // SHL  Vx   {, Vy}     ; quirked
    OpCode9xy0(u8, u8),      // SNE  Vx,  Vy
    OpCodeAnnn(u16),         // LD   I,   addr
    OpCodeBnnn(u16),         // JP   V0,  addr
    OpCodeCxnn(u8, u8),      // RND  Vx,  byte
    OpCodeDxyn(u8, u8, u8),  // DRW  Vx,  Vy, nibble 
    OpCodeEx9e(u8),          // SKP  Vx
    OpCodeExa1(u8),          // SKNP Vx
    OpCodeFx07(u8),          // LD   Vx,  DT
    OpCodeFx0a(u8),          // LD   Vx,  K
    OpCodeFx15(u8),          // LD   DT,  Vx
    OpCodeFx18(u8),          // LD   ST,  Vx
    OpCodeFx1e(u8),          // ADD  I,   Vx         ; quirked
    OpCodeFx29(u8),          // LD   F,   Vx
    OpCodeFx33(u8),          // LD   B,   Vx
    OpCodeFx55(u8),          // LD   [I], Vx         ; quirked
    OpCodeFx65(u8),          // LD   Vx,  [I]        ; quirked
    OpCodeInvalid(),
}

pub fn decode(instr: u16, quirk_flags: QuirkFlags) -> DecodedInstruction  {
    // An opcode is of the form NNNN, where the first N is the opcode-prefix 0-F.
    let prefix = instr >> 12;
    match prefix {
        
        // 0x0 prefixed opcodes.
        0x0 => {
            match instr {
                // 00E0
                0x00E0 => {
                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCode00e0(),
                        mnemonic: String::from("CLS")
                    }
                },

                // 00EE
                0x00EE => {
                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCode00ee(),
                        mnemonic: String::from("RET")
                    }
                },

                _ => invalid_instruction(instr)
            }
        },

        // 1NNN
        0x1 => {
            let addr = get_nnn(instr);

            DecodedInstruction {
                instr: instr,
                opcode: OpCode::OpCode1nnn(addr),
                mnemonic: format!("JP {:#05X}", addr)
            }
        },
        
        // 2NNN
        0x2 => {
            let addr = get_nnn(instr);

            DecodedInstruction {
                instr: instr,
                opcode: OpCode::OpCode2nnn(addr),
                mnemonic: format!("CALL {:#05X}", addr)
            }
        },
        
        // 3XNN
        0x3 => {
            let vx_idx = get_n2(instr);
            let val = get_nn(instr);

            DecodedInstruction {
                instr: instr,
                opcode: OpCode::OpCode3xnn(vx_idx, val),
                mnemonic: format!("SE V{:X}, {:#04X}", vx_idx, val)
            }
        },

        // 4XNN
        0x4 => {
            let vx_idx = get_n2(instr);
            let val = get_nn(instr);

                DecodedInstruction {
                    instr: instr,
                    opcode: OpCode::OpCode4xnn(vx_idx, val),
                    mnemonic: format!("SNE V{:X}, {:#04X}", vx_idx, val)
                }
        },
        
        // 0x5 prefixed opcodes.
        0x5 => {
            match get_n4(instr) {

                 // 5XY0
                0 => {
                    let vx_idx = get_n2(instr);
                    let vy_idx = get_n3(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCode5xy0(vx_idx, vy_idx),
                        mnemonic: format!("SE V{:X}, V{:X}", vx_idx, vy_idx),
                    }
                },

                _ => invalid_instruction(instr)
            }
        },

        // 6XNN
        0x6 => {
            
            let vx_idx = get_n2(instr);
            let val = get_nn(instr);

            DecodedInstruction {
                instr: instr,
                opcode: OpCode::OpCode6xnn(vx_idx, val),
                mnemonic: format!("LD V{:X}, {:#04X}", vx_idx, val)
            }
        },

        // 7XNN
        0x7 => {
            
            let vx_idx = get_n2(instr);
            let val = get_nn(instr);
            
            DecodedInstruction {
                instr: instr,
                opcode: OpCode::OpCode7xnn(vx_idx, val),
                mnemonic: format!("ADD V{:X}, {:#04X}", vx_idx, val)
            }
        },

        // 0x8 prefixed opcodes.
        0x8 => {
            match get_n4(instr) {

                // 8XYO
                0x0 => {
                    let vx_idx = get_n2(instr);
                    let vy_idx = get_n3(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCode8xy0(vx_idx, vy_idx),
                        mnemonic: format!("LD V{:X}, V{:X}", vx_idx, vy_idx),
                    }
                },

                // 8XY1
                0x1 => {
                    let vx_idx = get_n2(instr);
                    let vy_idx = get_n3(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCode8xy1(vx_idx, vy_idx),
                        mnemonic: format!("OR V{:X}, V{:X}", vx_idx, vy_idx),
                    }
                },

                // 8XY2
                0x2 => {
                    let vx_idx = get_n2(instr);
                    let vy_idx = get_n3(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCode8xy2(vx_idx, vy_idx),
                        mnemonic: format!("AND V{:X}, V{:X}", vx_idx, vy_idx),
                    }
                },

                // 8XY3
                0x3 => {
                    let vx_idx = get_n2(instr);
                    let vy_idx = get_n3(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCode8xy3(vx_idx, vy_idx),
                        mnemonic: format!("XOR V{:X}, V{:X}", vx_idx, vy_idx),
                    }
                },

                // 8XY4
                0x4 => {
                    let vx_idx = get_n2(instr);
                    let vy_idx = get_n3(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCode8xy4(vx_idx, vy_idx),
                        mnemonic: format!("ADD V{:X}, V{:X}", vx_idx, vy_idx),
                    }
                },

                // 8XY5
                0x5 => {
                    let vx_idx = get_n2(instr);
                    let vy_idx = get_n3(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCode8xy5(vx_idx, vy_idx),
                        mnemonic: format!("SUB V{:X}, V{:X}", vx_idx, vy_idx),
                    }
                },

                // 8XY6
                0x6 => {
                    let vx_idx = get_n2(instr);
                    let vy_idx = get_n3(instr);

                    let mnemonic = if quirk_flags.contains(QuirkFlags::QUIRK_8XY6) {
                        format!("SHR V{:X}, V{:X}", vx_idx, vy_idx)
                    } else {
                        format!("SHR V{:X}", vx_idx)
                    };

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCode8xy6(vx_idx, vy_idx),
                        mnemonic: mnemonic,
                    }
                },

                // 8XY7
                0x7 => {
                    let vx_idx = get_n2(instr);
                    let vy_idx = get_n3(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCode8xy7(vx_idx, vy_idx),
                        mnemonic: format!("SUBN V{:X}, V{:X}", vx_idx, vy_idx),
                    }
                },

                // 8XYE
                0xE => {
                    let vx_idx = get_n2(instr);
                    let vy_idx = get_n3(instr);

                    let mnemonic = if quirk_flags.contains(QuirkFlags::QUIRK_8XYE) {
                        format!("SHL V{:X}, V{:X}", vx_idx, vy_idx)
                    } else {
                        format!("SHL V{:X}", vx_idx)
                    };

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCode8xye(vx_idx, vy_idx),
                        mnemonic: mnemonic,
                    }
                },

                _ => invalid_instruction(instr)
            }
        },

        // 0x9 prefixed opcodes.
        0x9 => {
            match get_n4(instr) {

                // 9XY0
                0x0 => {
                    let vx_idx = get_n2(instr);
                    let vy_idx = get_n3(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCode9xy0(vx_idx, vy_idx),
                        mnemonic: format!("SNE V{:X}, V{:X}", vx_idx, vy_idx),
                    }
                },

                _ => invalid_instruction(instr)
            }
        },

        // ANNN
        0xA => {
            let addr = get_nnn(instr);

            DecodedInstruction {
                instr: instr,
                opcode: OpCode::OpCodeAnnn(addr),
                mnemonic: format!("LD I {:#05X}", addr)
            }
        },

        // BNNN
        0xB => {
            let addr = get_nnn(instr);

            DecodedInstruction {
                instr: instr,
                opcode: OpCode::OpCodeBnnn(addr),
                mnemonic: format!("JP V0, {:#05X}", addr)
            }
        },

        // CXNN
        0xC => {
            let vx_idx = get_n2(instr);
            let mask = get_nn(instr);

            DecodedInstruction {
                instr: instr,
                opcode: OpCode::OpCodeCxnn(vx_idx, mask),
                mnemonic: format!("RND V{:X}, {:#02X}", vx_idx, mask)
            }
        },

        // DXYN
        0xD => {
            let vx_idx = get_n2(instr);
            let vy_idx = get_n3(instr);

            let count = get_n4(instr);

            DecodedInstruction {
                instr: instr,
                opcode: OpCode::OpCodeDxyn(vx_idx, vy_idx, count),
                mnemonic: format!("DRW V{:X}, V{:X}, {:#01X}", vx_idx, vy_idx, count)
            }
        },

        // 0xE prefixed opcodes.
        0xE => {
            match get_nn(instr) {

                // EX9E
                0x9E => {
                    let vx_idx = get_n2(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCodeEx9e(vx_idx),
                        mnemonic: format!("SKP V{:X}", vx_idx)
                    }
                },

                // EXA1
                0xA1 => {
                    let vx_idx = get_n2(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCodeExa1(vx_idx),
                        mnemonic: format!("SKNP V{:X}", vx_idx)
                    }
                },

                _ => invalid_instruction(instr)
            }
        },

        // 0xF prefixed opcodes.
        _ => {
            match get_nn(instr) {
                
                // FX07
                0x07 => {
                    let vx_idx = get_n2(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCodeFx07(vx_idx),
                        mnemonic: format!("LD V{:X}, DT", vx_idx)
                    }
                },

                // FX0A
                0x0A => {
                    let vx_idx = get_n2(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCodeFx0a(vx_idx),
                        mnemonic: format!("LD V{:X}, K", vx_idx)
                    }
                },

                // FX15
                0x15 => {
                    let vx_idx = get_n2(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCodeFx15(vx_idx),
                        mnemonic: format!("LD DT, V{:X}", vx_idx)
                    }
                },

                // FX18
                0x18 => {
                    let vx_idx = get_n2(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCodeFx18(vx_idx),
                        mnemonic: format!("LD ST, V{:X}", vx_idx)
                    }
                },

                // FX1E
                0x1E => {
                    let vx_idx = get_n2(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCodeFx1e(vx_idx),
                        mnemonic: format!("ADD I, V{:X}", vx_idx)
                    }
                },

                0x29 => {
                    let vx_idx = get_n2(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCodeFx29(vx_idx),
                        mnemonic: format!("LD F, V{:X}", vx_idx)
                    }
                }

                // FX33
                0x33 => {
                    let vx_idx = get_n2(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCodeFx33(vx_idx),
                        mnemonic: format!("LD B, V{:X}", vx_idx)
                    }
                },

                // FX55
                0x55 => {
                    let vx_idx = get_n2(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCodeFx55(vx_idx),
                        mnemonic: format!("LD [I], V{:X}", vx_idx)
                    }
                },

                // FX65
                0x65 => {
                    let vx_idx = get_n2(instr);

                    DecodedInstruction {
                        instr: instr,
                        opcode: OpCode::OpCodeFx65(vx_idx),
                        mnemonic: format!("LD V{:X}, [I]", vx_idx)
                    }
                },

                _ => invalid_instruction(instr)
            }
        }
    }
}

#[inline(always)]
fn get_n2(instr: u16) -> u8 {
    ((0x0F00 & instr) >> 8) as u8
}

#[inline(always)]
fn get_n3(instr: u16) -> u8 {
    ((0x00F0 & instr) >> 4) as u8
}

#[inline(always)]
fn get_n4(instr: u16) -> u8 {
    (0x000F & instr) as u8
}

#[inline(always)]
fn get_nnn(instr: u16) -> u16 {
    0x0FFF & instr
}

#[inline(always)]
fn get_nn(instr: u16) -> u8 {
    (0x00FF & instr) as u8
}

#[inline(always)]
fn invalid_instruction(instr: u16) -> DecodedInstruction {
    DecodedInstruction {
        instr: instr,
        opcode: OpCode::OpCodeInvalid(),
        mnemonic: String::from(""),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn assert_decoded_instr(instr: u16, opcode: OpCode, mnemonic: String, decoded_instr: &DecodedInstruction) {
        let expected = DecodedInstruction {
            instr,
            opcode,
            mnemonic
        };

        assert_eq!(expected, *decoded_instr);
    }

    #[test]
    fn decode_00e0_test() {
        let decoded_instr = decode(0x00E0, QuirkFlags::NONE);
        assert_decoded_instr(0x00E0, OpCode::OpCode00e0(), "CLS".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_00ee_test() {
        let decoded_instr = decode(0x00EE, QuirkFlags::NONE);
        assert_decoded_instr(0x00EE, OpCode::OpCode00ee(), "RET".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_1nnn_test() {
        let decoded_instr = decode(0x123F, QuirkFlags::NONE);
        assert_decoded_instr(0x123F, OpCode::OpCode1nnn(0x23F), "JP 0x23F".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_2nnn_test() {
        let decoded_instr = decode(0x212F, QuirkFlags::NONE);
        assert_decoded_instr(0x212F, OpCode::OpCode2nnn(0x12F), "CALL 0x12F".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_3xnn_test() {
        let decoded_instr = decode(0x312F, QuirkFlags::NONE);
        assert_decoded_instr(0x312F, OpCode::OpCode3xnn(0x1, 0x2F), "SE V1, 0x2F".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_4xnn_test() {
        let decoded_instr = decode(0x412F, QuirkFlags::NONE);
        assert_decoded_instr(0x412F, OpCode::OpCode4xnn(0x1, 0x2F), "SNE V1, 0x2F".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_5xy0_test() {
        let decoded_instr = decode(0x51F0, QuirkFlags::NONE);
        assert_decoded_instr(0x51F0, OpCode::OpCode5xy0(0x1, 0xF), "SE V1, VF".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_6xnn_test() {
        let decoded_instr = decode(0x6123, QuirkFlags::NONE);
        assert_decoded_instr(0x6123, OpCode::OpCode6xnn(0x1, 0x23), "LD V1, 0x23".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_7xnn_test() {
        let decoded_instr = decode(0x712F, QuirkFlags::NONE);
        assert_decoded_instr(0x712F, OpCode::OpCode7xnn(0x1, 0x2F), "ADD V1, 0x2F".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_8xy0_test() {
        let decoded_instr = decode(0x81F0, QuirkFlags::NONE);
        assert_decoded_instr(0x81F0, OpCode::OpCode8xy0(0x1, 0xF), "LD V1, VF".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_8xy1_test() {
        let decoded_instr = decode(0x81F1, QuirkFlags::NONE);
        assert_decoded_instr(0x81F1, OpCode::OpCode8xy1(0x1, 0xF), "OR V1, VF".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_8xy2_test() {
        let decoded_instr = decode(0x81F2, QuirkFlags::NONE);
        assert_decoded_instr(0x81F2, OpCode::OpCode8xy2(0x1, 0xF), "AND V1, VF".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_8xy3_test() {
        let decoded_instr = decode(0x81F3, QuirkFlags::NONE);
        assert_decoded_instr(0x81F3, OpCode::OpCode8xy3(0x1, 0xF), "XOR V1, VF".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_8xy4_test() {
        let decoded_instr = decode(0x81F4, QuirkFlags::NONE);
        assert_decoded_instr(0x81F4, OpCode::OpCode8xy4(0x1, 0xF), "ADD V1, VF".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_8xy5_test() {
        let decoded_instr = decode(0x81F5, QuirkFlags::NONE);
        assert_decoded_instr(0x81F5, OpCode::OpCode8xy5(0x1, 0xF), "SUB V1, VF".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_8xy6_test() {
        let decoded_instr = decode(0x81F6, QuirkFlags::NONE);
        assert_decoded_instr(0x81F6, OpCode::OpCode8xy6(0x1, 0xF), "SHR V1".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_8xy6_quirk_mode_test() {
        let decoded_instr = decode(0x81F6, QuirkFlags::QUIRK_8XY6);
        assert_decoded_instr(0x81F6, OpCode::OpCode8xy6(0x1, 0xF), "SHR V1, VF".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_8xy7_test() {
        let decoded_instr = decode(0x81F7, QuirkFlags::NONE);
        assert_decoded_instr(0x81F7, OpCode::OpCode8xy7(0x1, 0xF), "SUBN V1, VF".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_8xye_test() {
        let decoded_instr = decode(0x81FE, QuirkFlags::NONE);
        assert_decoded_instr(0x81FE, OpCode::OpCode8xye(0x1, 0xF), "SHL V1".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_8xye_quirk_mode_test() {
        let decoded_instr = decode(0x81FE, QuirkFlags::QUIRK_8XYE);
        assert_decoded_instr(0x81FE, OpCode::OpCode8xye(0x1, 0xF), "SHL V1, VF".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_9xy0_test() {
        let decoded_instr = decode(0x91F0, QuirkFlags::NONE);
        assert_decoded_instr(0x91F0, OpCode::OpCode9xy0(0x1, 0xF), "SNE V1, VF".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_annn_test() {
        let decoded_instr = decode(0xA1CD, QuirkFlags::NONE);
        assert_decoded_instr(0xA1CD, OpCode::OpCodeAnnn(0x1CD), "LD I 0x1CD".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_bnnn_test() {
        let decoded_instr = decode(0xB1CD, QuirkFlags::NONE);
        assert_decoded_instr(0xB1CD, OpCode::OpCodeBnnn(0x1CD), "JP V0, 0x1CD".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_cxnn_test() {
        let decoded_instr = decode(0xC12F, QuirkFlags::NONE);
        assert_decoded_instr(0xC12F, OpCode::OpCodeCxnn(0x1, 0x2F), "RND V1, 0x2F".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_dxyn_test() {
        let decoded_instr = decode(0xD12F, QuirkFlags::NONE);
        assert_decoded_instr(0xD12F, OpCode::OpCodeDxyn(0x1, 0x2, 0xF), "DRW V1, V2, 0xF".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_ex9e_test() {
        let decoded_instr = decode(0xE19E, QuirkFlags::NONE);
        assert_decoded_instr(0xE19E, OpCode::OpCodeEx9e(0x1), "SKP V1".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_exa1_test() {
        let decoded_instr = decode(0xE1A1, QuirkFlags::NONE);
        assert_decoded_instr(0xE1A1, OpCode::OpCodeExa1(0x1), "SKNP V1".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_fx07_test() {
        let decoded_instr = decode(0xF107, QuirkFlags::NONE);
        assert_decoded_instr(0xF107, OpCode::OpCodeFx07(0x1), "LD V1, DT".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_fx0a_test() {
        let decoded_instr = decode(0xF10A, QuirkFlags::NONE);
        assert_decoded_instr(0xF10A, OpCode::OpCodeFx0a(0x1), "LD V1, K".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_fx15_test() {
        let decoded_instr = decode(0xF115, QuirkFlags::NONE);
        assert_decoded_instr(0xF115, OpCode::OpCodeFx15(0x1), "LD DT, V1".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_fx18_test() {
        let decoded_instr = decode(0xF118, QuirkFlags::NONE);
        assert_decoded_instr(0xF118, OpCode::OpCodeFx18(0x1), "LD ST, V1".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_fx1e_test() {
        let decoded_instr = decode(0xF11e, QuirkFlags::NONE);
        assert_decoded_instr(0xF11e, OpCode::OpCodeFx1e(0x1), "ADD I, V1".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_fx29_test() {
        let decoded_instr = decode(0xF129, QuirkFlags::NONE);
        assert_decoded_instr(0xF129, OpCode::OpCodeFx29(0x1), "LD F, V1".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_fx33_test() {
        let decoded_instr = decode(0xF133, QuirkFlags::NONE);
        assert_decoded_instr(0xF133, OpCode::OpCodeFx33(0x1), "LD B, V1".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_fx55_test() {
        let decoded_instr = decode(0xF155, QuirkFlags::NONE);
        assert_decoded_instr(0xF155, OpCode::OpCodeFx55(0x1), "LD [I], V1".to_string(), &decoded_instr)
    }

    #[test]
    fn decode_fx65_test() {
        let decoded_instr = decode(0xF165, QuirkFlags::NONE);
        assert_decoded_instr(0xF165, OpCode::OpCodeFx65(0x1), "LD V1, [I]".to_string(), &decoded_instr)
    }
}