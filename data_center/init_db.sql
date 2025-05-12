CREATE TABLE IF NOT EXISTS okx_trades (
    ts BIGINT NOT NULL,
    instrument_id TEXT,
    price DOUBLE PRECISION NOT NULL,
    size DOUBLE PRECISION NOT NULL,
    side BOOLEAN NOT NULL,
    order_count INT NOT NULL,
    PRIMARY KEY (instrument_id, ts)
);

