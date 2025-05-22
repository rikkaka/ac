use crate::data::Bbo;

impl Bbo {
    fn get_unbiased_price(&self) -> f64 {
        let best_bid_prc = self.best_bid.price;
        let best_bid_sz = self.best_bid.size;
        let best_ask_prc = self.best_ask.price;
        let best_ask_sz = self.best_ask.price;

        (best_bid_prc * best_ask_sz + best_ask_prc * best_bid_sz) / (best_bid_sz + best_ask_sz)
    }

    fn get_spread(&self) -> f64 {
        self.best_ask.price - self.best_bid.price
    }

    fn get_relevent_spread(&self) -> f64 {
        self.get_spread() / self.get_unbiased_price()
    }
}