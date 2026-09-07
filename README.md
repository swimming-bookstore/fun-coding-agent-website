# Fun coding agent website

Static WASM site for [Fun coding agent](https://github.com/swimming-bookstore/fun-coding-agent). Leptos CSR + Tailwind v4. The homepage runs a Fun-style agent in WASM (virtual files, zip download).

The browser cannot call xAI directly (CORS). Same-origin `/xai-auth` and `/xai-api` are proxied to xAI by `trunk serve` or `fun-site`.

```sh
# once
cargo install trunk
rustup target add wasm32-unknown-unknown
npm install

cd ~/fun-coding-agent-website
trunk serve
```

http://127.0.0.1:8080

Release (static files + proxy):

```sh
trunk build --release
cargo run --release --manifest-path host/Cargo.toml
```
