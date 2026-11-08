# Source-faithful TeX math patch

This branch starts at upstream markdown-rs 1.0.0, commit
`1506572f9b406431402928f3a8b3df0b4ae2d8f5` (MIT; see `license`). The equivalent
published archive has SHA-256
`a5cab8f2cadc416a82d2e783a1946388b31654d391d1c7d92cc1f03e295b1deb`.
The package remains `markdown` version `1.0.0`, compatible with GPUI's dependency.
The declared Rust version remains 1.56; regression diagnostics avoid newer format syntax.

## Syntax

The existing `constructs.math_text` and `constructs.math_flow` switches gate all
changes. No public option fields, Cargo features, AST/serde formats, or parser
compiler changes are introduced. With math off, parsing matches upstream.

* Inline `\(…\)` is tried before Markdown character escapes. Its content is raw
  TeX; backslashes pair with the next byte, so `\\)` cannot close it. The first
  unpaired `\)` closes. An unmatched opener stays a Markdown escape. Content may
  span lines within one paragraph. Code spans and link destinations are not math.
* After up to three spaces at a line start, a line containing only `\[` opens
  a display fence. Raw content follows until a line ending in an unpaired `\]`
  and optional horizontal whitespace. TeX can precede the closer on that line.
  An unclosed fence runs to the container/document end, like fenced code.
* Whole-line `\[…\]` and `$$…$$` are displays only if no prose follows the first
  unpaired closer. `\[1\] Smith` and mid-line `\[…\]` remain ordinary Markdown.
  Displays interrupt paragraphs and respect non-lazy container continuations.
* Raw flow is tried first: existing dollar fences, including `$$$`, are unchanged.
  Inline multi-dollar math remains available mid-paragraph. Indented/fenced code
  and fenced code labeled `math` are unchanged. `\\[2mm]` stays intact in TeX.
* Single-dollar openers must precede non-whitespace; closers must follow
  non-whitespace and must not precede an ASCII digit. Failed closer candidates
  remain data while scanning continues. Unicode whitespace is recognized.

The dollar policy deliberately changes upstream's whitespace-padded single-dollar
examples. Use `\(…\)` or inline multi-dollar math when that content is wanted.
A whole-line `$$…$$` deliberately changes from inline to display math. Upstream
math tests are updated to distinguish these cases from legacy inline behavior.

## Implementation ledger

* `construct/tex_math_text.rs` and `construct/tex_math_flow.rs`: new constructs,
  emitting the existing MathText / MathFlow event families and line endings.
* `construct/text.rs`, `construct/flow.rs`, `construct/mod.rs`, `state.rs`:
  registration, precedence, and dispatch.
* `construct/raw_text.rs`: dollar flanking policy and failed-scan memo.
* `tokenizer.rs`: dedicated opener/failed-scan/display fields, avoiding the
  shared scratch state used by other constructs.

`to_mdast.rs` and `to_html.rs` are unmodified. A narrow crate-level Clippy style
allowance preserves upstream AST compiler control flow on newer toolchains, and
an existing tokenizer debug equality assertion uses `debug_assert_eq!`. Math node spans cover the original
source including delimiters, without preprocessing or rewriting source bytes.
Failed inline scans are memoized per text tokenizer for linear behavior on
repeated unmatched openers. A closing-fence attempt never closes a content event
opened before the attempt: close it first and reopen it on failure.

## Regression tests

`tests/tex_math.rs` covers syntax, raw TeX/backslash parity, dollar/currency cases,
containers, code, link/table contexts, CRLF/tabs, HTML, disabled-construct golden
ASTs, repeated unmatched openers, and incremental last-block reparsing at chunk
sizes 1–16 and every character boundary. Run with debug assertions enabled.
The generic fixture in `tests/fixtures/tex-math.md` is self-contained: this fork
contains no application session records, provider replay, or private proof fixture.
Application-specific proof/copy/native-render tests remain in Zevria.

Validation commands (from this fork):

```sh
rtk cargo fmt --all -- --check
rtk cargo test --locked --all-features --workspace
rtk cargo clippy --locked --all-features --all-targets --workspace
```

Keep the patch isolated when updating upstream. Recheck the math-disabled AST
shapes, exact spans, HTML, containers and incremental boundaries; compare ordinary
Markdown with upstream as well. Drop the fork override once upstream offers the
same source-faithful syntax and dollar semantics.

The old SWC 22 MDX test adapter requires Serde's former private API. A checked-in
workspace `Cargo.lock` freezes its compatible test-tooling graph (Serde 1.0.219,
serde_json 1.0.140, vergen 9.0.6, camino 1.1.12, env_logger 0.11.6 and semver
1.0.26). CI checks/tests use `--locked`. No library or dev-dependency version
ranges are narrowed in the manifest: application consumers ignore this dependency
workspace's lockfile and can continue using current Serde. Updating this fork's
dev graph requires checking the old SWC adapter's compatibility first.

Incremental parsers should account for the inherent ambiguity of an unfinished
single-line display: a bare `\[` at EOF can become `\[x` (ordinary prose), then
`\[x\]` (a display), then `\[x\] tail` (prose again). A last-block-only append
model cannot always rejoin an already committed preceding paragraph when such
a construct retracts. The generic append fixture separates those same-line
forms with blank lines, and still exercises paragraph-interrupting bracket
fences without blank lines. Full-parse contract tests cover same-line
interruption and declined trailing prose separately.
