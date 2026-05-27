//! Geometry shared by all engine stages.

use crate::EngineUnit;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: EngineUnit,
    pub y: EngineUnit,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self {
            x: EngineUnit::new(x),
            y: EngineUnit::new(y),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: EngineUnit,
    pub y: EngineUnit,
    pub width: EngineUnit,
    pub height: EngineUnit,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x: EngineUnit::new(x),
            y: EngineUnit::new(y),
            width: EngineUnit::new(width),
            height: EngineUnit::new(height),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Insets {
    pub left: EngineUnit,
    pub right: EngineUnit,
    pub top: EngineUnit,
    pub bottom: EngineUnit,
}
