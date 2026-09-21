# Releasing

This project distributes via **GitHub Releases** (binaries, built by
[cargo-dist]), **crates.io**, **AUR**, and a **`.deb`** (built by [cargo-deb]).
The whole workspace is versioned as one block: every crate shares the
`[workspace.package]` version and they release together.

## One-time setup

```sh
cargo install cargo-dist cargo-deb

# Generate the release workflow (.github/workflows/release.yml) from the
# [workspace.metadata.dist] config; commit it.
dist init --yes
git add .github/workflows/release.yml Cargo.toml && git commit -m "ci: cargo-dist release workflow"
```

Repository secrets needed on GitHub:

- `CARGO_REGISTRY_TOKEN` — a crates.io API token; the `publish` workflow uses it to
  publish on tag. Set it with `gh secret set CARGO_REGISTRY_TOKEN`.
- `AUR_SSH_KEY` — the private half of a deploy key registered on the AUR account; the
  `aur` workflow pushes both packages with it. Without it that workflow still builds and
  validates each package, but skips the push.

## Before you tag: review the docs that drift

After any structural change (a new crate, a moved module, a renamed feature), walk
this list and fix what no longer matches the code. This is the step that is easy to
skip and expensive to miss:

- [ ] **Root [`README.md`](README.md)** — the tool table, the shared-library paragraph
      (which crates), Requirements, Install, the Documentation links.
- [ ] **Every crate README** — one per crate, they must all exist and be current:
      `crates/wlr-capture`, `crates/wlr-i18n`, `crates/wlr-chooser`, `crates/wlr-shot`,
      `crates/wlr-peek`, `crates/wlr-draw`, `crates/wlr-utils`. A **new crate** needs its
      own `README.md` *and* a `readme = "README.md"` line in its `Cargo.toml`.
- [ ] **A new binary** drifts in more places than a new crate, because nothing fails to
      build when one is missed: `[[bin]]` in its own crate *and* in the `wlr-utils`
      bundle (with the shim under `src/bin/`), the `assets` list of both `.deb`s, both
      `description`s, the root README's binary count and its Install / Uninstall
      sections, the crate table in `CONTRIBUTING.md`, and `docs/index.md`. Grep the tree
      for the binary it sits next to and answer every hit.
- [ ] **[`CONTRIBUTING.md`](CONTRIBUTING.md)** — the workspace crate table, the
      feature-combo list, the Translations and Themes sections.
- [ ] **[`COMPATIBILITY.md`](COMPATIBILITY.md)** — compositor floors / capability matrix.
      Revisit **every row**, not just the ones the release touched: compositors ship
      protocols between our releases, so a `❌` goes stale on its own. Check each
      project's current release notes, say which rows were verified at runtime and on
      which version, and leave the rest marked as inferred.
- [ ] **`docs/`** — `wlr-draw-keys.toml`, `themes/`, `index.md` (the showcase site).

Quick sanity greps (adjust to the change):

```sh
# every crate has a README and declares it
for d in crates/*/; do
  test -f "$d/README.md" || echo "MISSING README: $d"
  grep -q '^readme = ' "$d/Cargo.toml" || echo "MISSING readme field: $d/Cargo.toml"
done
grep 'for pkg in' .github/workflows/publish.yml   # every crate in the publish order
```

## Cutting a release `vX.Y.Z`

1. **Bump the toolchain** if a newer stable is out: `rustup update`, then set the same
   version in `rust-toolchain.toml`. That file is the single source of truth — CI, the
   `.deb` builds and a local checkout all resolve to it, so a green local `clippy` means
   a green CI `clippy`.
2. **Bump the version.** It lives in `[workspace.package]` **and** in each inter-crate
   dependency pin (the `version = "X.Y.Z"` next to `path = "../wlr-…"`). Every crate's
   own version inherits via `version.workspace = true`, but the tool crates pin the
   engine/i18n version explicitly, so those pins must move too. `cargo set-version X.Y.Z`
   (from `cargo-edit`) handles both; verify the pins and refresh `Cargo.lock`.
3. **Update [`CHANGELOG.md`](CHANGELOG.md)** — a `## X.Y.Z — YYYY-MM-DD` section
   (Added / Changed / Fixed), referencing the issues/PRs it closes. Commit
   (`chore(release): X.Y.Z`) as the last commit of the release PR.
4. **Tag and push:**
   ```sh
   git tag vX.Y.Z
   git push --tags
   ```
   - The `publish` workflow publishes each crate to **crates.io** in dependency order
     (`for pkg in …` in `.github/workflows/publish.yml`: a crate before anything that
     depends on it — currently
     `wlr-capture wlr-i18n wlr-chooser wlr-shot wlr-peek wlr-draw wlr-utils`).
   - The cargo-dist `release` workflow builds the binaries + installer and creates
     the GitHub Release.
   - `deb` and `aur` are **chained to `release`** (a `workflow_run` trigger), because
     both need the release it creates. They take the tag from the upstream run, so
     they check out that tag rather than the default branch.
   - The `deb` workflow builds the `.deb` per distro and attaches it to the release
     (only crates with `[package.metadata.deb]` ship there).
   - The `aur` workflow publishes both AUR packages from an Arch container: it rewrites
     `pkgver` from the tag, runs `updpkgsums`, regenerates `.SRCINFO` and pushes. The
     `pkgver` committed in `packaging/aur/` is only kept in sync for readability.
5. **Replay a failed tag workflow** without re-tagging: `deb` and `aur` take a
   `workflow_dispatch` with the tag as an input; `publish` takes one on the version
   currently in `Cargo.toml`.

## Checks before tagging

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build -p wlr-utils          # the bundle isn't in the default set
cargo check --locked              # Cargo.lock is up to date
```

When the engine changed, also spot-check its feature combos (see CONTRIBUTING).

[cargo-dist]: https://opensource.axo.dev/cargo-dist/
[cargo-deb]: https://github.com/kornelski/cargo-deb
