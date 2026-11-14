<img src="assets/icon.svg" width="64" height="64" alt="">

# civil-mentor

A desktop study companion for UPSC CSE, written in Rust with GPUI. See [PLAN.md](PLAN.md) for what it does, how it works and what is built so far.

## Run

```sh
cargo run
```

Needs Rust 1.89 or newer. The first build takes about two minutes.

## Test

```sh
cargo test
```

This also drives the app in an offscreen window and saves a screenshot of every screen to `target/shots/`.

## Content

Everything the app shows lives in `content/` as Markdown and TOML. Debug builds read it from disk on every start, so edit a file and restart to see the change. `cargo test` fails if a content file has a mistake and says which one.
