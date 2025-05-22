pub struct Order {
    kind: String,
    price: Option<f64>,
    size: f64,
    side: bool,
}

pub struct Position {
    side: bool,
    size: f64,
}

pub struct Portfolio {
    positions: Vec<Position>,
}

pub struct Trade {
    ts: i64,
    price: f64,
    size: f64,
    side: bool,
}

pub struct State {
    orders: Vec<Order>,
    portfolio: Portfolio,
}

impl State {

}

