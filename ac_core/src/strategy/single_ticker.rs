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

pub struct AccountState {
    orders: Vec<Order>,
    portfolio: Portfolio,
}

impl Order {
    // Check whether the order
    // fn check_deal_with_trade(&mut self, trade: Trade)
}

impl AccountState {
    fn renew_with_trade(&mut self, trade: Trade) {
        // if ticker
        todo!()
    }
}

