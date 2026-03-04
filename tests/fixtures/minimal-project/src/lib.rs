pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn distance(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

pub enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
}

impl Shape {
    pub fn area(&self) -> f64 {
        match self {
            Shape::Circle(r) => std::f64::consts::PI * r * r,
            Shape::Rectangle(w, h) => w * h,
        }
    }
}
