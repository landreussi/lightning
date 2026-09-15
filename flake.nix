{
  description = "An experimental API to consume BTC Lightning layer data";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};
      inherit (pkgs) lib;

      env = {
        MEMPOOL_ENDPOINT = "https://mempool.space/api/v1/lightning/nodes/rankings/connectivity";
        PGHOST = "localhost";
        PGPORT = "5432";
        PGUSER = "postgres";
        PGDATABASE = "lightning";
        RUST_LOG = "info";
      };

      # Substituted here rather than left as ${...} for dotenvy: nix knows the
      # values, so the file that lands in the checkout is already resolved.
      databaseUrl = with env; "postgres://${PGUSER}@${PGHOST}:${PGPORT}/${PGDATABASE}";

      envFile = pkgs.writeText "dot-env" (lib.concatStringsSep "\n" (
        lib.mapAttrsToList (name: value: "${name}=${value}") env
        ++ ["DATABASE_URL=${databaseUrl}" ""]
      ));
    in {
      devShells.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          rustup
          pkg-config
          openssl
          postgresql
          diesel-cli
        ];

        env = env;

        shellHook = ''
          # Generate dotenv
          install -m 644 ${envFile} .env
          . ./.env

          # Init db
          export PGDATA="$PWD/.pg"
          [ -s "$PGDATA/PG_VERSION" ] ||
            initdb --username="$PGUSER" --auth=trust --encoding=UTF8 --no-locale >/dev/null

          # Start db
          pg_ctl status >/dev/null 2>&1 ||
            pg_ctl start --wait --silent --log "$PGDATA/postgres.log" \
              --options "-p $PGPORT -k $PGDATA -h $PGHOST"

          # Create db
          diesel setup

          # Run migrations
          diesel migration run
        '';
      };
    });
}
