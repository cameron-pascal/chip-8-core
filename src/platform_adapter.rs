pub trait PlatformAdapter {
    fn play_sound(&mut self);
    fn pause_sound(&mut self);
    fn get_random_val(&self) -> u8;
}