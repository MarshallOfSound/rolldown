# @marshallofsound/rolldown

A fork of [rolldown](https://github.com/rolldown/rolldown) **1.2.5** carrying a
set of performance patches (and one codegen option) that are on their way
upstream. It exists so a large Vite 8 application can use them before the
corresponding rolldown / oxc releases ship; it is not a long-term fork.

Published as:

- `@marshallofsound/rolldown` — drop-in for `rolldown` (same JS API, same
  `dist/` layout and subpath exports),
- `@marshallofsound/rolldown-binding-<triple>` — the native binding, for
  `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
  `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc` and
  `aarch64-pc-windows-msvc` only.

Versions are `<upstream version>-mos.<n>`, e.g. `1.2.5-mos.3`.

## Using it under Vite

Vite 8 depends on `rolldown` by name, so alias it (yarn shown; pnpm
`overrides` / npm `overrides` work the same way):

```jsonc
// package.json
"resolutions": {
  "rolldown": "npm:@marshallofsound/rolldown@1.2.5-mos.3"
}
```

The aliased package's `optionalDependencies` pull in the matching
`@marshallofsound/rolldown-binding-*` for the host platform. rolldown's loader
only enforces an exact binding version when `NAPI_RS_ENFORCE_VERSION_CHECK`
is set, so nothing else is needed.

## What is different from upstream 1.2.5

All patches keep bundle output byte-identical unless stated.

| commit                                                                      | area           | effect                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| --------------------------------------------------------------------------- | -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `perf(binding): hand source/chunk code to JS as zero-copy external strings` | binding        | JS plugin hooks receive module source / chunk code as V8 external strings (Latin-1 for ASCII, one SIMD transcode to UTF-16 otherwise) instead of a UTF-8 decode + copy per call; the retained bytes are reported to V8 through `napi_adjust_external_memory`, and `OutputChunk.code` pins only the code bytes (not the chunk's sourcemap/module table). `ROLLDOWN_NO_EXTERNAL_STRINGS=1` opts out. Unix hosts only (symbol lookup via `dlsym`); elsewhere the copying path is used.                                                                                                                  |
| `perf: evaluate codeSplitting group test/name callbacks concurrently`       | generate stage | function-valued `codeSplitting.groups[].test/name` are evaluated up front, one callback function at a time in deterministic module order, instead of one awaited JS round trip per (module × group) inside the assignment loop.                                                                                                                                                                                                                                                                                                                                                                      |
| `perf: incremental cycle check in optimize_dynamic_entry_bits`              | generate stage | the static-cycle veto no longer rebuilds and DFS-es the whole atom→chunk graph per candidate.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `feat: ModuleInfo.hasTopLevelAwait`                                         | API            | new boolean on `ModuleInfo`; also teaches the scanner that a module-scope `await using` declaration is a top-level await.                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `perf: lazy bundle object and ModuleInfo fields on the JS side`             | JS glue        | `OutputChunk`/`OutputAsset` fields and `ModuleInfo` id lists are resolved on first access.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `perf(binding): compute RenderedModule.renderedLength natively`             | binding        | no code copy to read a length.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `feat: output.minify.codegen.asciiOnly`                                     | minifier       | new option; escapes non-ASCII characters in string literals, untagged template literals, regular expressions and identifiers so the output is 7-bit clean (like terser `ascii_only` / esbuild `charset: 'ascii'`; tagged templates and JSX element names are left as written). Implemented in a vendored `oxc_codegen` 0.146.0 (`vendor/`, wired through `[patch.crates-io]`), which **also changes the minifier's quote choice on a cost tie to a plain string literal instead of a template literal** — this one does change minified output (`import("./x.js")` instead of ``import(`./x.js`)``). |
| `perf: minify largest chunks first, one rayon task each`                    | generate stage | scheduling only.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `fix: derive asset reference ids from content hash`                         | file emitter   | reference ids for assets emitted without an explicit `fileName` were hashes of a thread-order-dependent emission counter, making Vite's `__VITE_ASSET__<id>__` placeholders (and anything hashed over pre-render chunk code, e.g. Sentry debug ids) nondeterministic across identical builds; now derived from the content hash the dedupe map already uses. Ids are internal/pre-render, so final bundle output is unchanged — just reproducible. Upstreamed as [rolldown#10797](https://github.com/rolldown/rolldown/pull/10797).                                                                  |

## Repository layout

- Branch `mos/1.2.x` = upstream `v1.2.5` (+1 upstream commit) + the patches
  above + this packaging. Upstream's package names are kept in the tree so
  rebases, workspace references and filters are untouched;
  `scripts/fork/prepare-release.mjs` renames at release time.
- Upstream's GitHub workflows are parked in `.github/workflows-upstream/`
  (not run); `.github/workflows/fork-release.yml` is the only workflow.

## Releasing

1. Actions → **Fork release** → _Run workflow_ on `mos/1.2.x`, `version` =
   `1.2.5-mos.N`. Leave _publish_ unchecked to build all five bindings and the
   node package and dry-run the publish; check it to publish. Publishing uses npm
   trusted publishing (OIDC): this repository and `fork-release.yml` are
   registered as the trusted publisher of all six packages, so there is no
   token secret; published versions carry provenance. The run starts with a
   preflight job that performs the OIDC token exchange for each package name
   and fails fast, before anything is built, if one is not configured.
2. Locally, the same steps are:

   ```bash
   node scripts/fork/prepare-release.mjs --version 1.2.5-mos.N
   pnpm --filter ./packages/rolldown run build-binding:release   # this platform's .node
   pnpm --filter ./packages/rolldown run build-node
   cd packages/rolldown && pnpm exec napi create-npm-dirs \
     && cp src/rolldown-binding.*.node artifacts/ 2>/dev/null; pnpm run artifacts \
     && NAPI_DRY_RUN=1 pnpm publish --dry-run --no-git-checks
   ```

## Rebasing onto a newer upstream release

```bash
git fetch origin --tags
git rebase --onto vX.Y.Z v1.2.5 mos/1.2.x     # then fix conflicts, mostly in
                                              # manual_code_splitting.rs / generate_stage
```

If upstream bumped oxc, re-vendor `oxc_codegen` / `oxc_minify_napi` at the new
version and re-apply the two small diffs in `vendor/` (see the asciiOnly
commit), or drop the vendoring once oxc ships `ascii_only`.
