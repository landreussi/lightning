# Lightning API

Persists and serves BTC Lightning node data, sourced from
[mempool.space](https://mempool.space)'s connectivity ranking.

```
GET /nodes                # every stored node, largest capacity first
GET /nodes/{public_key}   # one node, by its hex public key
GET /health
```

```json
[
  {
    "public_key": "03864ef025fde8fb587d989186ce6a4a186895ee44a926bfc370e2c366597a3f8f",
    "alias": "ACINQ",
    "capacity": "360.10516297",
    "first_seen": "2018-04-05T15:13:42Z"
  }
]
```

## Build tools & versions used

Rust 2024 edition (rustc 1.98), a Cargo workspace of three crates:

| crate     | what it is                                                |
|-----------|------------------------------------------------------------|
| `mempool` | lib: the mempool.space HTTP client, base URL per instance   |
| `api`     | lib + bin: the axum API, ports and adapters                 |
| `sync`    | bin: the cron job that refreshes the stored nodes           |

axum + tokio + tracing for the service, diesel (`diesel-async` over
`tokio-postgres`, deadpool) for Postgres, `rust_decimal` for capacities, `time`
for timestamps, `mockall` for the port mocks and `mockito` for the client's
tests. Nix flake for the toolchain and a throwaway Postgres.

## Steps to run the app

Both binaries and the tests read their configuration from `.env`. Under Nix the
shell writes it for you; without Nix, copy `.env.example` and fill it in.

| variable                                        | what it is                              |
|-------------------------------------------------|-----------------------------------------|
| `MEMPOOL_ENDPOINT`                              | the whole ranking URL, path included    |
| `PGHOST`, `PGPORT`, `PGUSER`, `PGDATABASE`      | the Postgres to use, in libpq's names   |
| `PGPASSWORD`                                    | only if the server wants one            |
| `DATABASE_URL`                                  | built from the PG* lines by substitution|
| `PORT`                                          | what the API listens on, 3000 by default|

`DATABASE_URL` is what the crates dial, and `dotenvy` substitutes the PG* lines
into it, so one edit moves both. Keep it below them and out of single quotes;
neither substitutes. Anything already exported wins over the file.

**With Nix** — `nix develop` writes `.env` itself, from the `settings` attribute
in `flake.nix`: that attribute is the single source for the connection details
and `DATABASE_URL` is derived from it, so the shell, the cluster and the crates
cannot disagree. It then runs `pg-start`, which initialises a Postgres cluster
under `.pg/` as those lines describe and starts it, and `diesel setup`, which
creates the database and applies every pending migration. Both are idempotent,
so re-entering the shell is a no-op. `pg-start` is on the shell's PATH too, for
a cluster stopped by hand; `pg_ctl stop` stops it, and `rm -rf .pg` is the whole
uninstall — the data is reproducible from the sync job. Point `PGHOST` at a host
that is not local and `pg-start` leaves it alone. The rewrite of `.env` is
claimed by the generated first line of the file: delete that line and
`nix develop` keeps its hands off your edits.

**Without Nix** — bring your own Postgres, put its URL in `.env`, and apply the
migrations with [`diesel_cli`](https://diesel.rs/guides/getting-started):

```sh
cp .env.example .env && $EDITOR .env
cargo install diesel_cli --no-default-features --features postgres
diesel setup                   # creates the database, then migrates it
```

Then, either way:

```sh
cargo run -p sync              # fill the database from mempool.space
cargo run -p api               # serve it on $PORT
cargo test --workspace         # the repository tests use the configured database
```

The sync job runs once and exits, so cron owns the schedule:

```crontab
*/15 * * * * cd /srv/api && ./sync
```

## What was the reason for your focus? What problems were you trying to solve?

Keeping the domain independent of what it is plugged into. The handlers talk to
two ports — `NodeRepository` and `NodeSource` — and never to Postgres or to
mempool.space, so the API, the sync job and the tests wire the same use cases to
different adapters. That is what makes the handler tests plain unit tests over
`mockall` doubles, and what makes the client testable against `mockito` on its
own.

The other focus was storing the data exactly. Capacities are exact to the
satoshi, so they are `NUMERIC(16, 8)` in Postgres and `rust_decimal::Decimal` in
Rust, and they are serialised as JSON strings — a float would round them. A node
identity is a fixed 33-byte compressed public key, stored as `BYTEA` rather than
`TEXT`/`VARCHAR`: the raw bytes are half the size of the hex text in both the
heap tuple and the primary key index, and a `CHECK (octet_length = 33)` makes a
malformed key unstorable, which a length-bounded `VARCHAR` could not promise.

## How long did you spend on this project?

About half a day, most of it on the data modelling and the tests rather than on
the wiring.

## Did you make any trade-offs for this project? What would you have done differently with more time?

- **No transactions.** Every write is a single statement, so Postgres' implicit
  per-statement transaction is enough; a second write in one use case would need
  them back.
- **The sync job upserts, it never deletes.** A node that drops out of the
  ranking keeps its last known row. Reporting "what is in the top 100 *now*"
  would need a sync run to be a snapshot, not a merge.
- **The repository tests run against the dev database** instead of a per-test
  one, so they leave rows behind and can't run against a shared instance.
- No pagination, no caching, no auth: the dataset is 100 rows behind a public
  API.

With more time: pagination and filtering on `GET /nodes`, the `channels` and
`country` fields the upstream also offers, and a snapshot table so capacity over
time is answerable.

## What do you think is the weakest part of your project?

The sync job's failure mode is coarse. A fetch either parses into nodes or is
dropped with a warning, and the whole batch is written in one statement — so a
single bad row is invisible in the API, and a write failure loses the whole run
until cron fires again. Per-node results and a retry would be the fix.

## Is there any other information you'd like us to know?

`api/src/schema.rs` is `diesel print-schema` output and is checked against
the migrations; regenerate it with `diesel print-schema > api/src/schema.rs`
after adding one.
