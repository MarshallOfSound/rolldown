#!/usr/bin/env node
// Rewrites packages/rolldown/package.json for publishing this fork as
// `@marshallofsound/rolldown` (+ `@marshallofsound/rolldown-binding-<triple>`).
//
// The repository keeps upstream's package name (`rolldown`) so that workspace
// references, filters and rebases stay untouched; the rename happens only in
// release CI (and can be run locally to produce the same tarballs).
//
// Usage:
//   node scripts/fork/prepare-release.mjs --version 1.2.5-mos.3 [--binding-only]
//
//   --binding-only  only set napi.packageName / napi.targets (enough for the
//                   jobs that merely build: the generated loader must require
//                   the fork's binding packages), keep name/version as-is.
//   --version       required unless --binding-only; must start with the
//                   upstream version this branch is based on.
//   --targets a,b   override the binding target list (local single-platform
//                   packaging tests; CI always builds the full list).
//   --print-packages  print the npm package names a release publishes (main
//                   package first, then one binding package per target) and
//                   exit without touching package.json.
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const FORK = {
  scope: '@marshallofsound',
  name: '@marshallofsound/rolldown',
  bindingName: '@marshallofsound/rolldown-binding',
  repo: 'https://github.com/MarshallOfSound/rolldown',
  // Platforms this fork builds and publishes bindings for. Anything else falls
  // through rolldown's loader to "Cannot find native binding".
  targets: [
    'x86_64-unknown-linux-gnu',
    'aarch64-unknown-linux-gnu',
    'aarch64-apple-darwin',
    'x86_64-apple-darwin',
    'x86_64-pc-windows-msvc',
    'aarch64-pc-windows-msvc',
  ],
};

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const value = (name) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
};

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const pkgPath = path.join(root, 'packages/rolldown/package.json');

if (flag('--print-packages')) {
  const require = createRequire(pkgPath);
  const { parseTriple } = await import(require.resolve('@napi-rs/cli'));
  const targets = value('--targets') ? value('--targets').split(',') : FORK.targets;
  console.log(FORK.name);
  for (const t of targets) console.log(`${FORK.bindingName}-${parseTriple(t).platformArchABI}`);
  process.exit(0);
}
const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
const upstreamVersion = pkg.version;

pkg.napi.packageName = FORK.bindingName;
pkg.napi.targets = value('--targets') ? value('--targets').split(',') : FORK.targets;

if (!flag('--binding-only')) {
  const version = value('--version') ?? process.env.FORK_VERSION;
  if (!version) {
    console.error('prepare-release: --version <x.y.z-mos.N> (or FORK_VERSION) is required');
    process.exit(1);
  }
  if (!version.startsWith(`${upstreamVersion}-`) && version !== upstreamVersion) {
    console.error(
      `prepare-release: version ${version} must be ${upstreamVersion} or ${upstreamVersion}-<suffix> ` +
        `(the upstream release this branch is based on)`,
    );
    process.exit(1);
  }
  pkg.name = FORK.name;
  pkg.version = version;
  pkg.description = `${pkg.description} (Fork of rolldown ${upstreamVersion} with performance patches; see FORK.md.)`;
  pkg.homepage = `${FORK.repo}/blob/HEAD/FORK.md`;
  pkg.repository = { type: 'git', url: `git+${FORK.repo}.git`, directory: 'packages/rolldown' };
  pkg.publishConfig = {
    ...pkg.publishConfig,
    access: 'public',
    registry: 'https://registry.npmjs.org/',
  };
  // napi pre-publish fills these in for FORK.targets at publish time; make sure
  // no upstream @rolldown/binding-* entries survive.
  delete pkg.optionalDependencies;
}

fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n');
console.log(
  `prepare-release: ${pkg.name}@${pkg.version}, bindings ${pkg.napi.packageName}-{${pkg.napi.targets.join(',')}}`,
);
