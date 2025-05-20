CREATE TABLE IF NOT EXISTS okx_trades (
    ts BIGINT NOT NULL,
    instrument_id TEXT,
    trade_id TEXT UNIQUE,
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
    order_count_bid INT NOT NULL,
    PRIMARY KEY (ts, instrument_id)
);
CREATE INDEX IF NOT EXISTS idx_okx_bbo ON okx_bbo (ts, instrument_id);
CREATE INDEX IF NOT EXISTS idx_okx_bbo_ts ON okx_bbo (ts);

CREATE OR REPLACE FUNCTION prevent_out_of_order_insert()
RETURNS TRIGGER AS $$
DECLARE
    max_ts BIGINT;
BEGIN
    EXECUTE format('SELECT MAX(ts) FROM %I', TG_TABLE_NAME)
    INTO max_ts;

    IF NEW.ts <= max_ts THEN
        RAISE EXCEPTION 'Insertion rejected in table %: ts value (%) < max ts (%)',
                        TG_TABLE_NAME, NEW.ts, max_ts;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER trg_prevent_insert_okx_trades
BEFORE INSERT ON okx_trades
FOR EACH ROW
EXECUTE FUNCTION prevent_out_of_order_insert();

CREATE OR REPLACE TRIGGER trg_prevent_insert_okx_bbo
BEFORE INSERT ON okx_bbo
FOR EACH ROW
EXECUTE FUNCTION prevent_out_of_order_insert();