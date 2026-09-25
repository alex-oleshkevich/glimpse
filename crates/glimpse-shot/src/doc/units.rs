#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale(pub f64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Logical(pub f64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferPx(pub i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImagePx(pub i32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point<U> {
    pub x: U,
    pub y: U,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect<U> {
    pub x: U,
    pub y: U,
    pub width: U,
    pub height: U,
}
