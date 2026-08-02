# jumakl-extractor — Project Overview & Target Structure

## What this project does

Extracts product data (price and/or stock) from multiple suppliers, each of
which may expose data in a different format (XML feed, CSV feed, or requiring
custom per-supplier processing). The extracted data is collected, analyzed
(compared against our own pricing/stock logic), written to a mass CSV export,
and a report is sent out — currently planned as email, later possibly also
pushed to a custom API server.

Two modes exist: **price** and **stock**. They run against mostly the same
suppliers and share almost all plumbing (fetching, config, HTTP, logging,
reporting) — they differ only in which fields get extracted and how the
extracted data is analyzed and exported.

## Key decision: one binary, not two

Price and stock modes stay in a **single binary**, selected at runtime via a
CLI flag (`--extract stock|price`). They share too much (fetching, config
schema, HTTP client, logging, reporting) to justify splitting into separate
executables. Mode-specific behavior is expressed as a **trait/strategy**, not
duplicated code or a second binary.

Revisit this only if the future "send to custom API server" piece becomes an
actual standalone long-running service — that's a different runtime shape
(server vs. batch CLI) and would deserve its own binary/crate at that point,
likely in the same Cargo workspace.

## Key decision: independent dispatch axes, resolved via trait objects

There are things that vary per source/run: **format** (xml, csv, custom, ...),
**kind** (stock, price), and (once implemented) **report channel** (email,
api). The rule for this codebase: these never nest into a combined match
(`match kind { .. match format { .. } .. }`). Instead each axis gets its own
trait, and exactly one "boundary" `match` resolves an enum/kind into a trait
object for that axis:

- `FormatConfig::parser(&self) -> &dyn sources::FormatParser` (in `config.rs`)
  is the only place that matches on format.
- `pipeline::new_handler(kind) -> Box<dyn Handler>` (in `pipeline/mod.rs`)
  is the only place that matches on kind for analysis.
- `output::create_writer(kind) -> Result<Box<dyn OutputWriter>>` (in
  `output/mod.rs`) is the only place that matches on kind for output writing.

Everything downstream (`sources::parse_source`, `pipeline::run_source`) just
calls trait methods — zero matches, so nothing nests. Adding a third format,
kind, or writer means adding one file + one arm in its own boundary match; it
never touches the other axes.

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
materializing a full feed's worth of records in memory just to hand them off.

Implemented for XML (`sources/xml.rs`, real logic, not a stub): walks
`quick_xml` events, matches each text/CDATA node's enclosing tag against the
configured field selectors, and calls `on_record` once per closed
`record_tag` element. CSV (`sources/csv.rs`) is still a `todo!()` stub.

**CDATA**: handled as its own `Event::CData` branch alongside `Event::Text`.
CDATA content must **not** be passed through entity-unescaping (it's raw by
XML spec — a literal `&amp;` inside CDATA must stay `&amp;`), so it only goes
through `.decode()`, skipping the `quick_xml::escape::unescape()` step that
`Event::Text` needs. The "which field does this tag map to, coerce, store"
logic is shared between both branches via a small `apply_field()` helper.

**Zero-copy tag comparisons — a real footgun hit while implementing this**:
`Reader::from_str(content)` holds the whole document in memory, so
`reader.read_event()` (not `read_event_into(&mut buf)`) yields events that
borrow directly from `content` — tag name comparisons *within the same loop
iteration* (e.g. checking a `Start`/`End` tag against `record_tag`) can stay
zero-copy byte-slice comparisons. But `BytesStart::name()`'s returned data
turned out to be tied to that iteration's own event binding, not to
`content`'s full lifetime — so `current_tag`, which must survive *into a
later iteration* (a `Start` tag read now is looked up again when its child
`Text`/`CData` arrives), cannot be a borrowed `&[u8]`. It's an owned
`Vec<u8>` (a small copy per tag, but still no UTF-8 validation, unlike the
original `String::from_utf8_lossy(...).into_owned()` approach).

## Key decision: `Record` is a small fixed struct, not a generic map

Originally this was a generic `HashMap<String, Value>` (any field, any
shape). In practice, config validation (see below) already guarantees every
source's `fields` contains exactly `"ean"` plus one kind-specific value field
(`"stock"` or `"price"`) — so a generic map bought indirection without real
flexibility. `Record` is now:

```rust
pub struct Record {
    pub ean: String,
    pub value: RecordValue,
}

pub enum RecordValue {
    Stock(i64),
    Price(f64),
}

impl RecordValue {
    pub fn as_stock(&self) -> Option<i64> { .. }
    pub fn as_price(&self) -> Option<f64> { .. }
}
```

Format parsers build this directly by recognizing the reserved field *names*
`"ean"`/`"stock"`/`"price"` (not by knowing `ExtractKind` — a config file only
ever defines one of `stock`/`price` in the first place, so the parser never
needs to ask "which kind is this run for"). Any other configured field (e.g.
a `"name"` column) is parsed but currently has nowhere to go and is dropped —
acceptable since `Record`'s only consumers are `Handler` impls that only ever
want `ean` + the one value.

`Value` (the old generic type: `String`/`Integer`/`Decimal`/`Date`) still
exists, but demoted to an intermediate: `coerce(raw, field_type) -> Value`
respects each field's configured `FieldType` from TOML, and
`Value::into_string()`/`into_i64()`/`into_f64()` convert that into whatever
concrete primitive the destination `Record` field actually needs (handling
e.g. a `price` field configured as `type = "integer"` in TOML but needing to
end up as `f64`).

## Key decision: `Handler` both analyzes *and* derives the export shape

```rust
pub trait Handler {
    fn on_record(&mut self, record: &Record) -> Result<Option<WriteRow>>;
    fn finish(&mut self, source_name: &str) -> SourceReport;
}
```

`on_record` fires per record: it reads the raw `Record` (analyze/accumulate
whatever's needed — e.g. a running count, or price mismatches against our own
logic) **and** decides what should actually be exported. `Ok(None)` means
skip writing this record on purpose; `Ok(Some(row))` hands back a `WriteRow`
— the *derived*, ready-to-export shape. This is deliberately where any
editing happens (e.g. `PriceHandler` computing `buy`/`sell`/`fix` from one
raw scraped price), **not** in the writer — see the next section for why
that split exists. `finish` fires once after a source's records are
exhausted and returns the finished `SourceReport` (see below). Because
handlers accumulate state between calls, a **fresh instance is created per
source** via `new_handler` — they are not shared/global singletons.

`StockHandler::on_record` is a pure passthrough (`WriteRow::Stock { n }`,
straight from the parsed value). `PriceHandler::on_record` is still a
`todo!()` — it needs to pull the scraped price, compare it against internal
pricing logic, and derive `buy`/`sell`/`fix`.

## Key decision: `output/` is a mirror of `sources/` — writing is its own axis

Writing was tempting to fold into `Handler` (since `Handler` already knows
the kind-specific business logic), but that would collapse two things that
vary independently: kind and output *format*. Kept as a separate trait, in
its own directory, following the exact same shape as `sources/`:

```rust
// output/mod.rs
pub enum WriteRow {
    Stock { n: i64 },
    Price { buy: f64, sell: f64, fix: f64 },
}

pub trait OutputWriter {
    fn write_record(&mut self, ean: &str, row: &WriteRow, shortname: &str, prefix: &str) -> Result<()>;
}

pub fn create_writer(kind: ExtractKind) -> Result<Box<dyn OutputWriter>> { .. } // the one boundary match
```

`WriteRow` is what a `Handler` has already derived and decided is ready to
export — writers never see raw `Record`s or business logic, only the final
shape to print. Concretely this means `Handler` and `OutputWriter` compose in
`pipeline::run_source` rather than one owning the other: `on_record` runs
first; only if it returns `Some(row)` does `run_source` hand that row to the
writer.

`output/csv.rs` currently has **two concrete writer types**,
`StockCsvWriter`/`PriceCsvWriter` (not one `CsvWriter` matching on
`record.value` per call) — since a writer created for one kind will *only
ever* see that kind's `WriteRow` variant, each writer's `write_record` uses a
`let-else` to unwrap its own variant with no per-call branching at all, and
`output::create_writer`'s single match is what picks which concrete type to
construct. Each row includes a `DATE` column (`chrono::Local::now()` in
`dd.mm.yyyy`, computed once at writer construction, not per row).

## Key decision: `SourceReport` — implemented, mail/API sending is not

```rust
pub struct SourceReport {
    pub supplier: String,
    pub records: usize,
    pub errors: Vec<String>,
}
```

Built per-source and destined for the eventual mail report. Two rules that
took a couple of iterations to land on:

- **The `Handler` owns `records`.** It already tracks its own record count
  internally (needed for its own logging), so `finish()` constructs and
  returns the `SourceReport` directly — `run_source` does not keep a
  duplicate counter.
- **`run_source` never returns `Result` and never silently drops an error.**
  It always returns a `SourceReport`, even for a source whose fetch failed
  outright (an empty report with the error recorded in `errors`). A fetch
  failure, a parse failure, a per-record write failure, or a per-record
  `on_record` failure are all pushed into `errors` as formatted strings (in
  addition to being `tracing::warn!`/`error!`-logged at the point they
  happen, for real-time visibility) — a single bad record no longer aborts
  processing for the rest of the source, and a single bad source no longer
  means "nothing to report," it means "a report saying what went wrong."
  `pipeline::run` collects one `Vec<SourceReport>` per kind's run.

**Not yet implemented**: actually sending these anywhere. `reports` is built
and currently just logged locally at the end of `run()`, then dropped. Plan
(unchanged from before, not yet built): a `report/` module with a `Reporter`
trait (`EmailReporter`/`ApiReporter`), resolved via its own boundary match —
a fourth independent axis, same pattern as format/kind/output. No database:
keep reports in memory for the run; optionally persist to a local `reports/`
folder (mirroring `paths::output_folder_path()`) before attempting to send,
as a durable audit trail / retry buffer. Open question: does sending happen
once per kind, or once for the whole program after both kinds complete?

## Config additions

- `SourceConfig` gained `shortname: String` and `prefix: String` — used in
  the exported CSV row (`{shortname};{prefix} - {ean};...`), not part of
  extraction itself.
- `SourceConfig.auth: Option<AuthConfig>` (`{ username, password }`), a
  single optional `[sources.auth]` sub-table (not `[[sources.auth]]` — a
  source has at most one credential pair, so no array-of-tables). Wired into
  `webrequest::fetch(client, url, auth: Option<&AuthConfig>)`, which adds
  HTTP Basic auth via `request.basic_auth(...)` when present.
- `Config::load` now takes `kind: ExtractKind` and calls `Config::validate`
  after parsing: every source's `fields` must contain `"ean"`, plus
  `"stock"` or `"price"` depending on `kind` — fails fast at load time
  instead of producing incomplete `Record`s later. The kind→field-name
  mapping lives in exactly one place, `ExtractKind::value_field()` (in
  `cli.rs`), reused by both `config.rs`'s validation and anywhere else that
  needs it — not re-matched locally.

## Target module structure

```
src/
  main.rs                  # wiring only: parse CLI, load config, call pipeline, exit code
  cli.rs                   # CLI parsing; ExtractKind::value_field() - the one kind->fieldname mapping
  config.rs                 # Config::load/validate, SourceConfig, AuthConfig, FormatConfig
                              # (+ ::parser() boundary match), FieldMapping, FieldType
  logging.rs                # logging setup
  paths.rs                  # config_folder_path(), output_folder_path()
  webrequest.rs              # fetch(client, url, auth) -> String, HTTP Basic auth when auth is Some

  sources/                  # "how do I turn raw text into Records"
    mod.rs                 # Record/RecordValue/Value, FormatParser trait, parse_source() (no match)
    csv.rs                 # impl FormatParser for CsvConfig - todo!() stub
    xml.rs                 # impl FormatParser for XmlConfig - implemented, incl. CDATA
    custom/                 # not yet built - see "custom per-supplier processing" below
      mod.rs
      <supplier>.rs

  pipeline/
    mod.rs                 # Handler trait, SourceReport, new_handler() boundary match,
                              # run(), run_source() (no match, never returns Err)
    stock.rs                # StockHandler: implemented (passthrough WriteRow::Stock)
    price.rs                # PriceHandler: todo!() - needs buy/sell/fix derivation + mismatch logic

  output/                   # "how do I turn a WriteRow into an export file" - mirrors sources/
    mod.rs                  # WriteRow, OutputWriter trait, create_writer() boundary match
    csv.rs                  # StockCsvWriter, PriceCsvWriter - both implemented

  report/                   # not yet built
    mod.rs                  # Reporter trait (own boundary match, fourth axis)
    email.rs                # EmailReporter
    api_client.rs            # ApiReporter (future)

config/
  sources.stock.toml         # per-mode source definitions (see below re: merging)
  sources.price.toml
```

### Custom per-supplier processing (planned, not built)

For sources whose feed needs supplier-specific handling beyond generic
xml/csv: add `FormatConfig::Custom(CustomConfig)` (`format = "custom"`,
holding a `supplier: String`), and a new `sources/custom/mod.rs` implementing
`FormatParser for CustomConfig` — its `parse()` does the *only* new match,
`self.custom.supplier.as_str()` → picks a supplier-specific function, one
file per supplier (`custom/automax.rs`, ...). This slots in as a third
`FormatConfig` arm; nothing downstream changes. Not a registry/dynamic
lookup unless there's a real reason (e.g. suppliers added without
recompiling) — a plain `match` is enough for a handful of suppliers.

### Config file: merge stock/price into one file (still planned, not done)

`sources.stock.toml` and `sources.price.toml` currently duplicate the same
suppliers/URLs and differ only in field selectors — this is a maintenance
trap (a URL change has to happen in two places and can drift). Structure:
one file per supplier with connection info once, and field mappings scoped
per mode underneath it (`sources.fields.stock`, `sources.fields.price`).

### Config file style conventions (decided)

- Keep field mappings as a `[sources.fields]` sub-table with **one line per
  field**, not a single multiline inline table. Better git-diff-friendliness
  (adding a field is a one-line diff) and better portability — this
  project's `toml` crate happens to target spec 1.1.0 (which allows
  multiline inline tables), but most other TOML tooling still assumes 1.0.0,
  where that's invalid syntax.
- Cosmetic indentation of `[sources.xml]` / `[sources.fields]` under their
  `[[sources]]` block (purely visual — TOML ignores leading whitespace) is
  safe and recommended for readability: `.githooks/pre-commit`'s
  `cargo fmt --check` only touches `.rs` / `Cargo.toml`, never
  `config/*.toml`. Not yet applied to the existing config files.

## Suggested build order

1. **Config**: merge stock/price TOML into one schema with mode-scoped
   field blocks. *(not started)*
2. ~~Domain model (`model.rs`)~~ — superseded; `Record`/`RecordValue` (fixed
   struct, see above) fills this role.
3. **Format parsers** — XML ✅ implemented (incl. CDATA). CSV still `todo!()`.
4. **Pipeline wiring** — ✅ done: `pipeline::run` loops sources, never
   aborts the whole run on one bad source, collects a `SourceReport` per
   source either way.
5. **Handler (mode) trait** — ✅ done as a streaming visitor that also
   derives the export shape (`on_record` → `Option<WriteRow>`). Stock
   implemented as a passthrough; Price still `todo!()` (needs the actual
   buy/sell/fix logic).
6. **Output writing** — ✅ done: `output/` mirrors `sources/`,
   `StockCsvWriter`/`PriceCsvWriter` both implemented.
7. **Reporting collection** — ✅ done: `SourceReport` built per source,
   never dropped on failure. **Sending** (`report/`, `Reporter` trait,
   email/API) — *(not started)*.
8. **Storage + analysis**: persist last run's snapshot per source (JSON to
   start) and diff against the new run to detect price/stock changes; this
   is what real `PriceHandler::on_record` mismatch detection needs.
   *(not started)*
9. **Custom per-supplier processing**: `sources/custom/` per the plan above,
   once a supplier actually needs it. *(not started, not needed yet)*
10. **API client + multiple summary formats**: added last, once reporting is
    stable — additive, shouldn't require touching earlier stages.

## Guiding principles while building this out

- Never let dispatch axes (format, kind, output writer, and eventually
  reporter) nest into a combined match — one boundary match per axis,
  resolving to a trait object or concrete type, and nothing downstream
  matches on them again.
- Keep mode differences (price vs. stock) expressed as trait implementations
  operating on shared data (`Record`), not as separate code paths or
  duplicated config.
- Handlers may transform/derive data (`Record` → `WriteRow`) — writers must
  stay free of business logic, only knowing how to serialize whatever shape
  they're handed.
- Prefer streaming (callback/visitor) over collect-then-process when a step
  could otherwise buffer an entire feed in memory for no reason. Watch for
  borrowed-parser-event lifetimes not actually spanning as long as you'd
  expect across loop iterations (see the XML zero-copy note above) — verify
  rather than assume.
- Keep one supplier's failure, or one record's failure, from aborting the
  whole run — collect errors (into `SourceReport`) and keep going, don't
  panic/return early.
- Don't build the custom-supplier framework, extra summary formats, or a
  database before a concrete need actually forces it — add modules when the
  next real requirement arrives, following the structure above.
- Treat `config/*.toml` as the only place URLs, selectors, and supplier
  specifics live — code should stay generic over supplier details.
