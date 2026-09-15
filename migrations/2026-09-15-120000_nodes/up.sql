CREATE TABLE nodes (
    public_key BYTEA PRIMARY KEY CHECK (octet_length(public_key) = 33),
    alias TEXT NOT NULL,
    capacity NUMERIC(16, 8) NOT NULL,
    first_seen TIMESTAMPTZ NOT NULL,
    synced_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX nodes_capacity_idx ON nodes (capacity DESC);
CREATE INDEX nodes_first_seen_idx ON nodes (first_seen);
CREATE INDEX nodes_synced_at_idx ON nodes (synced_at);
