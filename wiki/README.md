# Wiki

The wiki is built with [mdbook](https://github.com/rust-lang/mdBook) and hosted on GitHub Pages.

To build the wiki without installing mdbook, run

```shell
cd wiki && cargo run --bin wiki
```

The built wiki will be in `wiki/book`.

To preview the wiki locally, install [mdbook](https://github.com/rust-lang/mdBook) and [mdbook-alerts](https://github.com/lambdalisue/rs-mdbook-alerts) and run

```shell
cd wiki && mdbook serve
```

A preview of the wiki will be available at `http://localhost:3000`.
