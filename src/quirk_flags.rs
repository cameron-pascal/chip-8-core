use bitflags::*;

bitflags! {
    pub struct QuirkFlags : u8 {
        const NONE = 0x00;
        const QUIRK_8XY6 = 0x01;
        const QUIRK_8XYE = 0x02;
        const QUIRK_FX1E = 0x04;
        const QUIRK_FX55 = 0x08;
        const QUIRK_FX65 = 0x10;
    }
}