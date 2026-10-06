# Releasing

This project distributes via **GitHub Releases** (binaries, built by
[cargo-dist]), **crates.io**, **AUR**, and a **`.deb`** (built by [cargo-deb]).
The whole workspace is versioned as one block: every crate shares the
`[workspace.package]` version and they release together.

## One-time setup

```sh
cargo install cargo-dist cargo-deb

# Generate the release workflow (.github/workflows/release.yml) from the [dist]
# config in dist-workspace.toml; commit it.
dist init --yes
git add .github/workflows/release.yml dist-workspace.toml && git commit -m "ci: cargo-dist release workflow"
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
      `crates/wlr-capture`, `crates/wlr-config`, `crates/wlr-i18n`, `crates/wlr-chooser`,
      `crates/wlr-shot`, `crates/wlr-peek`, `crates/wlr-draw`, `crates/wlr-utils`. A **new
      crate** needs its own `README.md` *and* a `readme = "README.md"` line in its
      `Cargo.toml`.
- [ ] **A new binary** drifts in more places than a new crate, because nothing fails to
      build when one is missed: `[[bin]]` in its own crate *and* in the `wlr-utils`
      bundle (with the shim under `src/bin/`), the `.deb`'s `assets` list and both its
      `extended-description`s, `BINS` in `packaging/install.sh`, the loop of
      `packaging/check-version.sh`, the root README's binary count and its Install /
      Uninstall sections, the crate table in `CONTRIBUTING.md`, and `docs/index.md`. Grep
      the tree for the binary it sits next to and answer every hit.
- [ ] **[`CONTRIBUTING.md`](CONTRIBUTING.md)** — the workspace crate table, the
      feature-combo list, the Translations and Themes sections.
- [ ] **[`COMPATIBILITY.md`](COMPATIBILITY.md)** — compositor floors / capability matrix.
      Revisit **every row**, not just the ones the release touched: compositors ship
      protocols between our releases, so a `❌` goes stale on its own. Check each
      project's current release notes, say which rows were verified at runtime and on
      which version, and leave the rest marked as inferred.
- [ ] **`docs/`** — `config.toml`, `themes/`, `index.md` (the showcase site).

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
   version in `rust-toolchain.toml`. CI, the `.deb` builds and a local checkout all
   resolve to it, so a green local `clippy` means a green CI `clippy`; only the AUR
   package builds with Arch's own Rust.
2. **Upgrade the dependencies, then the minimum Rust.** `cargo upgrade --incompatible
   --ignore-rust-version` (without the flag, cargo-edit holds back what needs a newer Rust
   than ours), fix what breaks, then measure the highest `rust-version` the dependencies
   declare:
   ```sh
   cargo metadata --format-version 1 | jq -r '[.packages[] | select(.source and .rust_version) | .rust_version] | max_by(split(".") | map(tonumber))'
   ```
   Set it as `rust-version` in `[workspace.package]`, and where the docs name it (the
   README's build prerequisites, `docs/index.md`, the bundle's README), with whether
   Debian's `trixie-backports` still has that Rust. Left
   below the real floor, it makes `resolver = "3"` hold back later `cargo update`s.
3. **Bump the version.** It lives in `[workspace.package]` **and** in each inter-crate
   dependency pin (the `version = "X.Y.Z"` next to `path = "../wlr-…"`). Every crate's
   own version inherits via `version.workspace = true`, but every crate that depends on
   another pins its version explicitly — the tools, and `wlr-capture` on `wlr-config`,
   `wlr-config` on `wlr-i18n` — so those pins must move too. `cargo set-version X.Y.Z`
   (from `cargo-edit`) handles both; verify the pins and refresh `Cargo.lock`.
4. **Update [`CHANGELOG.md`](CHANGELOG.md)** — a `## X.Y.Z — YYYY-MM-DD` section
   (Added / Changed / Fixed / Deprecated / Breaking), referencing the issues/PRs it
   closes. Commit (`chore(release): X.Y.Z`) as the last commit of the release PR.
5. **Tag and push:**
   ```sh
   git tag vX.Y.Z
   git push --tags
   ```
   - The `publish` workflow publishes each crate to **crates.io** in dependency order
     (`for pkg in …` in `.github/workflows/publish.yml`: a crate before anything that
     depends on it — currently
     `wlr-i18n wlr-config wlr-capture wlr-chooser wlr-shot wlr-peek wlr-draw wlr-utils`).
   - The cargo-dist `release` workflow builds the binaries + installer and creates
     the GitHub Release.
   - `deb` and `aur` are **chained to `release`** (a `workflow_run` trigger), because
     both need the release it creates. They take the tag from the upstream run, so
     they check out that tag rather than the default branch.
   - The `deb` workflow builds the suite's `.deb` (`cargo deb -p wlr-utils`) per distro
     and attaches it to the release.
   - The `aur` workflow publishes both AUR packages from an Arch container: it rewrites
     `pkgver` from the tag, runs `updpkgsums`, regenerates `.SRCINFO` and pushes. The
     `pkgver` committed in `packaging/aur/` is only kept in sync for readability.
   - Every binary built off a git checkout reports its commit (`X.Y.Z-N-gHASH`); only
     a build of the exact tag reports `X.Y.Z`. `packaging/check-version.sh` enforces
     it: `deb` and `aur` (source package) run it on what they built before shipping it,
     and `release-check`, also chained to `release`, runs it on the published
     cargo-dist archive.
6. **Replay a failed tag workflow** without re-tagging: `deb`, `aur` and
   `release-check` take a `workflow_dispatch` with the tag as an input; `publish` takes
   one on the version currently in `Cargo.toml`.

## Checks before tagging

```sh
cargo check --locked              # Cargo.lock is up to date (first: the rest rewrites it)
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build -p wlr-utils          # the bundle isn't in the default set
```

When the engine changed, also spot-check its feature combos (see CONTRIBUTING).

[cargo-dist]: https://opensource.axo.dev/cargo-dist/
[cargo-deb]: https://github.com/kornelski/cargo-deb
