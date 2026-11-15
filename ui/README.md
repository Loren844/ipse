# The interface

The interface of Ipse, written with Svelte (decision 0013) and served by `ipse ui` on this computer only (decision 0012).

Installing Ipse does not need Node: the built files in `build/` are committed and embedded in the `ipse` binary when it is compiled. Only changing the interface needs Node 22.

```sh
cd ui
npm ci
npm run build                 # writes build/, embedded at the next cargo build
cargo run -p ipse -- ui --vault <a test memory>
npm run check                 # types and Svelte checks
npm test                      # the browser tests, against target/debug/ipse
```

- Commit `build/` with the sources: CI builds again and fails when they differ.
- The API's TypeScript types in `src/lib/types/` are written from the Rust types by `cargo test -p ipse`; commit them too.
- Nothing is fetched from the network: fonts and scripts are part of the build.
- Words on screen live in `src/lib/i18n.svelte.ts`, in French and English.
