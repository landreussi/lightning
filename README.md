# Lightning API

This is a toy-project made for a tech evaluation step for a job.

Cargo workspace breakdown:
 - `mempool` is the lib crate responsible for interacting with the mempool API;
 - `api` is the binary crate that serves the node data. it also has a lib that contains types and db interactions;
 - `sync` is the binary crate that syncs the mempool API data - it is designed to be called by an crontab job;

## Build tools & versions used

 - Cargo & rustc nightly (as per `rust-toolchain.toml` file);
 - Nix (optional, but I used).

## Steps to run the app

- Without nix:
  - Install rustup and cargo;
  - Ensure you have a postgresql daemon running that attends to the configuration in .env;
  - Install Diesel CLI utility `cargo install diesel_cli`;
  - Enter `api` directory;
  - Run `diesel setup`;
  - Run `diesel migration run`;
  - Return to the root directory;

- With nix:
  - Run `nix develop`;
  
- `cargo run -p sync` runs the sync job;
- `cargo run -p api` to serve the API;
- `curl -s localhost:3000/nodes | jq` collects all the nodes;
- `curl -s -D - localhost:3000/nodes` to stream the nodes;

## What was the reason for your focus? What problems were you trying to solve?

I tried to implement ports and adapters pattern and DDD to make the "kernel" of this system testable
and agnostic to implementation details (such as which web server I'll use, or which database, or the nodes data-source).

Also tried to be efficient as I could responding the node list as a stream, so we don't need to wait the system collect all the node list
It is a more scalable solution in case the database becomes really big.

## How long did you spend on this project?

1 day

## Did you make any trade-offs for this project? What would you have done differently with more time?

I wouldn't really use PostgreSQL for this purpose, as it haves only one entity we could go with a cheaper and lightweight db.
I know you guys uses PostgreSQL with diesel, this is why I chose that stack to build this :D
Handler layer seems to be boilerplate, but is really useful when the service evolves.

## What do you think is the weakest part of your project?

The sync job could use a stream client, it would be way better, but once it is designed to be called as a CRON job I think it is fine as it is.

## Is there any other information you'd like us to know?

- The `.env` file is purposefully commited for this purpose in sake of simplicity, but I won't do this in prod at all :D
- The nix flake produces the `.env` file, to be compatible with non-nix users.
- I enabled `NO_SEQ_SCAN` to ensure the DB isn't performing a sequential scan locally, forcing myself and others to create indexes whenever we could!
