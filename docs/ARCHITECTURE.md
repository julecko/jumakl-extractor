# jumakl-extractor — Project Overview & Target Structure

## What this project does

Extracts product data (price and/or stock) from multiple suppliers, each of
which may expose data in a different format (XML feed, CSV feed, or requiring
a custom webscraper). The extracted data is collected, analyzed (compared
against previous results / against our own pricing logic), and a report is
sent out — currently planned as email, later possibly also pushed to a
custom API server. Summaries of the analysis should eventually be generated
in multiple output formats.

Two modes exist: **price** and **stock**. They run against mostly the same
suppliers and share almost all plumbing (fetching, config, HTTP, logging,
reporting) — they differ only in which fields get extracted and how the
extracted data is analyzed.

## Key decision: one binary, not two

Price and stock modes stay in a **single binary**, selected at runtime via a
CLI flag (already implemented as `--extract stock|price`). They share too
much (fetching, config schema, HTTP client, logging, reporting) to justify
splitting into separate executables. Mode-specific behavior is expressed as
a **trait/strategy**, not duplicated code or a second binary.

Revisit this only if the future "send to custom API server" piece becomes an
actual standalone long-running service — that's a different runtime shape
(server vs. batch CLI) and would deserve its own binary/crate at that point,
likely in the same Cargo workspace.

## Key decision: two independent dispatch axes, resolved via trait objects

There are two orthogonal things that vary per source: **format** (xml, csv,
html, ...) and **kind** (stock, price). The rule for this codebase: these
never nest into a combined match (`match kind { .. match format { .. } .. }`).
Instead each axis gets its own trait, and exactly one "boundary" `match`
resolves an enum into a trait object for that axis:

- `FormatConfig::parser(&self) -> &dyn sources::FormatParser` (in `config.rs`)
  is the only place that matches on format.
- `pipeline::new_handler(kind) -> Box<dyn Handler>` (in `pipeline/mod.rs`)
  is the only place that matches on kind.

Everything downstream (`sources::parse_source`, `pipeline::run_source`) just
calls trait methods — zero matches, so nothing nests. Adding a third format
or a third kind means adding one file + one arm in its own boundary match;
it never touches the other axis.

## Key decision: streaming (push-based) parsing, not collect-then-return

`FormatParser::parse` does **not** return `Vec<Record>`. It takes a callback:

```rust
fn parse(
    &self,
    content: &str,
    fields: &HashMap<String, FieldMapping>,
    on_record: &mut dyn FnMut(Record) -> Result<()>,
) -> Result<()>;
```

and invokes `on_record` once per parsed record as it's produced (this maps
naturally onto `quick_xml`'s own push-style event loop). This avoids ever
materializing a full feed's worth of records in memory just to hand them off
— the consumer (`Handler`) decides what's worth keeping.

## Key decision: `Record`/`Value` as the generic intermediate shape

Every format produces, and every handler consumes, the same generic type:

```rust
pub enum Value { String(String), Integer(i64), Decimal(f64), Date(String) }
pub type Record = HashMap<String, Value>;
```

keyed by the field names defined in the source's config (`fields.<name>.selector`,
`fields.<name>.type`). This fully decouples format code from mode code —
neither knows the other exists — without needing a bespoke domain model
(`ProductRecord`, `Ean`, `Price`, `StockLevel`, ...) up front. Handlers pull
out just the keys they care about (e.g. `record.get("sku")`) and match on the
`Value` variant they expect.

Revisit this (introduce real domain types in a `model.rs`) only if the
`HashMap<String, Value>` indirection actually causes pain — e.g. if a lot of
repeated `match`-on-`Value` boilerplate builds up across handlers, or fields
need validation/units beyond what `FieldType` expresses.

## Key decision: `Handler` is a streaming visitor, not a batch analyzer

```rust
pub trait Handler {
    fn on_record(&mut self, record: Record) -> Result<()>;
    fn finish(&mut self, source_name: &str) -> Result<()>;
}
```

`on_record` fires per record (analyze / accumulate whatever's needed —
e.g. a running count, or price mismatches against our own logic). `finish`
fires once after a source's records are exhausted (write the result,
eventually build the report for that source). Because handlers accumulate
state between calls, a **fresh instance is created per source** via
`new_handler` — they are not shared/global singletons.

## Target module structure

```
src/
  main.rs                  # wiring only: parse CLI, load config, call pipeline, exit code
  cli.rs                   # CLI argument parsing
  config.rs                 # Config::load, SourceConfig, FormatConfig (+ ::parser() boundary match),
                              # FieldMapping, FieldType
  logging.rs                # logging setup
  paths.rs                  # path resolution
  webrequest.rs              # fetch(client, url) -> String

  sources/                  # "how do I turn raw text into generic Records"
    mod.rs                 # Record/Value, FormatParser trait, parse_source() (no match)
    csv.rs                 # impl FormatParser for CsvConfig
    xml.rs                 # impl FormatParser for XmlConfig (quick_xml)
    scraper/                # not yet built — see build order, item 8
      mod.rs
      <supplier>.rs

  pipeline/
    mod.rs                 # Handler trait, new_handler() boundary match, run(), run_source() (no match)
    stock.rs                # StockHandler: Handler impl, accumulates e.g. record_count
    price.rs                # PriceHandler: Handler impl, accumulates e.g. record_count + mismatches

  report/                   # not yet built — see build order, item 7
    mod.rs                  # SourceReport, ReportDetails; Reporter trait (own boundary match, third axis)
    email.rs                # EmailReporter
    api_client.rs            # ApiReporter (future)

config/
  sources.stock.toml         # per-mode source definitions (see below re: merging)
  sources.price.toml
```

### Config file: merge stock/price into one file (still planned, not done)

`sources.stock.toml` and `sources.price.toml` currently duplicate the same
suppliers/URLs and differ only in field selectors — this is a maintenance
trap (a URL change has to happen in two places and can drift). Structure:
one file per supplier with connection info once, and field mappings scoped
per mode underneath it (`sources.fields.stock`, `sources.fields.price`).
Actual URLs/selectors live only in the config files themselves — not
duplicated here since they change independently of this document.

### Config file style conventions (decided)

- Keep field mappings as a `[sources.fields]` sub-table with **one line per
  field**, not a single multiline inline table. Better git-diff-friendliness
  (adding a field is a one-line diff) and better portability — this
  project's `toml` crate happens to target spec 1.1.0 (which allows
  multiline inline tables), but most other TOML tooling still assumes 1.0.0,
  where that's invalid syntax.
- Adding cosmetic indentation of `[sources.xml]` / `[sources.fields]` under
  their `[[sources]]` block (purely visual — TOML ignores leading whitespace)
  is safe and recommended for readability: `.githooks/pre-commit`'s
  `cargo fmt --check` only touches `.rs` / `Cargo.toml`, never `config/*.toml`.
  Not yet applied to the existing config files.

## Report shape (decided, not yet implemented)

`Handler::finish` should return a report instead of just `Result<()>`:

```rust
struct SourceReport {
    supplier: String,
    record_count: usize,
    details: ReportDetails, // Stock, or Price { mismatches: Vec<PriceMismatch> }
}
```

`pipeline::run` collects `Vec<SourceReport>` across a kind's sources.
Sending is its own `Reporter` trait (`EmailReporter` / `ApiReporter`),
resolved via a single boundary match off a new config enum — a third
independent axis alongside format and kind, following the same pattern.

Storage: no database for now — that solves a query-history problem this
project doesn't have yet. Keep reports in memory for the run; optionally
persist them to a local `reports/` folder (mirroring
`paths::config_folder_path()`) before attempting to send, as a durable
audit trail / retry buffer if the send fails.

Open question, not yet decided: does sending happen once per kind (stock
run finishes → send stock report; price run finishes → send price report),
or once for the whole program after both kinds complete? Leaning per-kind
since stock and price already run as separate passes with separate configs,
but revisit when actually implementing `report/`.

## Suggested build order

1. **Config**: merge stock/price TOML into one schema with mode-scoped
   field blocks. *(not started)*
2. ~~Domain model (`model.rs`)~~ — superseded by the generic `Record`/`Value`
   decision above; revisit only if that indirection starts to hurt.
3. **Format parsers** — ✅ done: `FormatParser` trait dispatched from
   `FormatConfig::parser()`, `sources/xml.rs` and `sources/csv.rs`
   implement it directly on `XmlConfig`/`CsvConfig`. Parsing bodies
   themselves are still `todo!()` stubs.
4. **Pipeline wiring** — ✅ done: `pipeline::run` loops sources, isolates
   per-source errors (log + continue, doesn't abort the whole run).
5. **Handler (mode) trait** — ✅ done, as a streaming visitor
   (`on_record`/`finish`) rather than a batch `field_mappings()`/`analyze()`
   pair. `StockHandler`/`PriceHandler` exist with `record_count` state as a
   placeholder; the actual field extraction and analysis are still `todo!()`.
6. **Storage + analysis**: persist last run's snapshot per source (JSON to
   start) and diff against the new run to detect price/stock changes; this
   is what a real `PriceHandler::on_record` needs in order to detect
   mismatches. *(not started)*
7. **Reporting**: implement `report/` per the shape decided above
   (`SourceReport`, `Reporter` trait, `email.rs`). *(not started)*
8. **Custom scrapers**: add `sources/scraper` trait + registry once a
   supplier actually needs one (don't build this speculatively).
9. **API client + multiple summary formats**: added last, once the above
   pipeline is stable — these are additive and shouldn't require touching
   earlier stages.

## Guiding principles while building this out

- Never let two dispatch axes (format, kind, and now reporter) nest into a
  combined match — one boundary match per axis, resolving to a trait object,
  and nothing downstream matches on them again.
- Keep mode differences (price vs. stock) expressed as trait implementations
  operating on shared data (`Record`), not as separate code paths or
  duplicated config.
- Prefer streaming (callback/visitor) over collect-then-process when a step
  could otherwise buffer an entire feed in memory for no reason.
- Keep one supplier's failure from aborting the whole run — collect errors
  per source and report them, don't panic/return early on the first one.
- Don't build the custom-scraper framework, extra summary formats, or a
  database before a concrete need actually forces it — add modules when the
  next real requirement arrives, following the structure above.
- Treat `config/*.toml` as the only place URLs, selectors, and supplier
  specifics live — code should stay generic over supplier details.
