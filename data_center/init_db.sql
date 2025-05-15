CREATE TABLE IF NOT EXISTS okx_trades (
    ts BIGINT NOT NULL,
    instrument_id TEXT,
    price DOUBLE PRECISION NOT NULL,
    size DOUBLE PRECISION NOT NULL,
    side BOOLEAN NOT NULL,
    order_count INT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_okx_trades ON okx_trades (ts, instrument_id);
CREATE INDEX IF NOT EXISTS idx_okx_trades_ts ON okx_trades (ts);

CREATE TABLE IF NOT EXISTS okx_bbo (
    ts BIGINT NOT NULL,
    instrument_id TEXT,
    price_ask DOUBLE PRECISION NOT NULL,
    size_ask DOUBLE PRECISION NOT NULL,
    order_count_ask INT NOT NULL,
    price_bid DOUBLE PRECISION NOT NULL,
    size_bid DOUBLE PRECISION NOT NULL,
    order_count_bid INT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_okx_bbo ON okx_bbo (ts, instrument_id);
CREATE INDEX IF NOT EXISTS idx_okx_bbo_ts ON okx_bbo (ts);