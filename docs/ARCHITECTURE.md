# jumakl-extractor — Project Overview & Target Structure

## What this project does

Extracts product data (price and/or stock) from multiple suppliers, each of
which may expose data in a different format (XML feed, CSV feed, or requiring
a custom webscraper). The extracted data is collected, analyzed (compared
against previous results), and a report is sent out — currently by email,
later possibly also pushed to a custom API server. Summaries of the analysis
should eventually be generated in multiple output formats.

Two modes exist: **price** and **stock**. They run against mostly the same
suppliers and share almost all plumbing (fetching, config, HTTP, logging,
reporting) — they differ only in which fields get extracted and how the
extracted data is analyzed.

## Key decision: one binary, not two

Price and stock modes stay in a **single binary**, selected at runtime via a
CLI flag (already implemented as `--extract stock|price`). They share too
much (fetching, config schema, HTTP client, logging, reporting) to justify
splitting into separate executables. Mode-specific behavior should be
expressed as a **trait/strategy**, not duplicated code or a second binary.

Revisit this only if the future "send to custom API server" piece becomes an
actual standalone long-running service — that's a different runtime shape
(server vs. batch CLI) and would deserve its own binary/crate at that point,
likely in the same Cargo workspace.

## Target module structure

```
src/
  main.rs                  # wiring only: parse CLI, load config, call pipeline, exit code
  cli.rs                   # CLI argument parsing
  logging.rs                # logging setup
  paths.rs                 # (renamed from helpers.rs) path resolution

  config/
    mod.rs                 # Config::load, top-level struct
    source.rs               # SourceConfig, FormatConfig, FieldMapping

  model.rs                  # domain types: ProductRecord, Ean, Price, StockLevel
                              # — decoupled from the config/serde shapes

  sources/                  # "how do I get raw records out of a supplier"
    mod.rs                 # Extractor trait: fetch() -> RawData, parse(RawData) -> Vec<ProductRecord>
    http.rs                # (renamed from webrequest.rs) client w/ timeout, retry, user-agent
    xml.rs
    csv.rs
    scraper/
      mod.rs               # Scraper trait + registry keyed by supplier id
      <supplier>.rs        # one file per supplier needing custom scraping logic

  pipeline/
    mod.rs                 # orchestrates: for each source -> extract -> analyze -> collect results
                              # isolates per-source errors so one bad feed doesn't kill the whole run
    mode.rs                # Mode trait: field_mappings(), analyze() — Stock and Price implement this
    stock.rs
    price.rs

  analysis.rs                # diff against previous snapshot
                              # (price changed / stock depleted / new product)
  storage.rs                  # persist + load last snapshot per source
                              # (start: JSON on disk; later: sqlite if history/queries needed)

  report/
    mod.rs                   # Report trait: render(&AnalysisResult) -> Output
                              # Sender trait: send(Output)
    email.rs                 # email sender
    api_client.rs             # future: push results to a custom API server
    summary/
      mod.rs
      text.rs
      html.rs
      csv.rs                 # add more formats here later without touching pipeline

config/
  sources.toml               # merged price+stock source definitions (see below)
```

### Config file: merge stock/price into one file

`sources.stock.toml` and `sources.price.toml` currently duplicate the same
suppliers/URLs and differ only in field selectors — this is a maintenance
trap (a URL change has to happen in two places and can drift). Structure:
one file per supplier with connection info once, and field mappings scoped
per mode underneath it (`sources.fields.stock`, `sources.fields.price`).
Actual URLs/selectors live only in the config files themselves — not
duplicated here since they change independently of this document.

## Suggested build order

1. **Config**: merge stock/price TOML into one schema with mode-scoped
   field blocks; split `config.rs` into `config/mod.rs` + `config/source.rs`.
2. **Domain model**: introduce `model.rs` with `ProductRecord` and friends,
   so parsers don't hand config structs directly to the pipeline.
3. **Extractor trait + XML/CSV implementations**: move `webrequest.rs`
   logic into `sources/http.rs`, add `sources/xml.rs` and `sources/csv.rs`
   behind a common `Extractor` trait dispatched from `FormatConfig`.
4. **Pipeline wiring**: implement `pipeline::run` to loop over sources,
   extract, and isolate per-source errors (log + continue, don't abort the
   whole run on one supplier failing).
5. **Mode trait**: implement `Stock`/`Price` as `Mode` implementations
   selecting which field block to use and how to analyze results.
6. **Storage + analysis**: persist last run's snapshot per source (JSON to
   start) and diff against the new run to detect price/stock changes.
7. **Reporting**: implement `report::email` to send a run summary
   (successes, failures, notable changes).
8. **Custom scrapers**: add `sources/scraper` trait + registry once a
   supplier actually needs one (don't build this speculatively).
9. **API client + multiple summary formats**: added last, once the above
   pipeline is stable — these are additive (`report::api_client`,
   `report::summary::*`) and shouldn't require touching earlier stages.

## Guiding principles while building this out

- Keep mode differences (price vs. stock) expressed as trait implementations
  operating on shared data, not as separate code paths or duplicated config.
- Keep one supplier's failure from aborting the whole run — collect errors
  per source and report them, don't panic/return early on the first one.
- Don't build the custom-scraper framework or extra summary formats before
  a concrete supplier/format actually needs them — add modules when the
  next real requirement arrives, following the structure above.
- Treat `config/*.toml` as the only place URLs, selectors, and supplier
  specifics live — code should stay generic over supplier details.
