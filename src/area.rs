use std::hash::Hash;

pub struct TextArea<F: Eq + Hash + Copy> {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub text: String,
    pub font: F,
    pub size: f32,
    pub line_height_factor: f32,
    pub top_offset: f32,
    pub left_offset: f32,
}
