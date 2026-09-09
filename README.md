# tree-cleaner

Interactive terminal explorer and cleaner for disk usage, written in Rust.

Walks your filesystem like a tree: the branch you are looking at is followed first and to
the leaves, everything else fills in around it. Shows what weighs, lets you delete it,
and knows where developer machines hide their gigabytes (Docker, `node_modules`, virtual
environments, Cargo targets, caches).

```
╭ tree-cleaner v0.2.0 ─────────────────────── ⠹ scanning 735K files · 245K dirs · 89K files/s · 8/8 thr · queue 703 ╮
│  1 Dashboard   2 Explorer   3 Heaviest   4 Cleaner                                                  allocated · GB │
│╭ ~/Documents ─────────────────────────────────────────── 312 GB · 1.2M files · 45K dirs  ⠹ scanning · 4 pending ╮│
││    # Name                              Size      %                        Items State                          ││
││ ▶  1 ▸ Projetos+                    120 GB    38% ████████████▏           234K ✓                               ││
││    2 ▸ Faculdade                   85.0 GB    27% ████████▏               120K ⠋ scanning                      ││
││    3 · backup.dmg                  20.0 GB   6.4% ██▏                                                          ││
││    4 ▸ Photos Library              15.2 GB   4.9% █▏                      1.2M ≡ dense                         ││
││    5 ▸ Mail                             —      —                             — ⊘ denied                        ││
│╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯│
│⏎ open · ⌫ up · space mark · d delete · T heaviest · r rescan · a size mode · f files · +/- rows · o reveal · : cmd │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
```

## Install

### Homebrew (macOS and Linux)

```bash
brew install lf-bf/tap/tree-cleaner
```

That one command taps [lf-bf/homebrew-tap](https://github.com/lf-bf/homebrew-tap), trusts
that single formula (Homebrew 6 requires trust before it loads a non-official formula) and
installs the prebuilt binary of the latest release: macOS on Apple silicon or Intel, Linux
on x86_64. It finishes in a second or two without building anything. Afterwards the short
name works too, so upgrades are `brew update && brew upgrade tree-cleaner`.

On a platform without a published binary, `brew install --HEAD lf-bf/tap/tree-cleaner`
builds from source instead (needs the `rust` formula), as does Cargo below.

### Cargo

```bash
cargo install --path .
```

Requires Rust 1.85 or newer to build. Runs on macOS and Linux, in any terminal
(truecolor when available, 16 colours otherwise, `NO_COLOR` respected). The binaries the
formula downloads are attached to every
[GitHub release](https://github.com/lf-bf/tree-cleaner/releases), so they can also be
fetched by hand.

## Use

```bash
tree-cleaner                    # dashboard + explorer at ~
tree-cleaner ~/Documents        # explorer at a directory
tree-cleaner -j 4 /             # four scanner threads, start at /
tree-cleaner measure ~/Library  # headless: measure and print a summary (also --json)
tree-cleaner categories         # what the cleaner knows about
tree-cleaner config init        # write ~/.config/tree-cleaner/config.toml
```

### Screens

| Key | Screen | What it shows |
| --- | --- | --- |
| `1` | Dashboard | Mounted volumes with usage bars, live breakdown of `/`, scanner health. |
| `2` | Explorer | Children of the current directory, largest first, with share bars, item counts and state. |
| `3` | Heaviest | The N largest files anywhere below the current directory (`T` starts it). |
| `4` | Cleaner | Reclaimable space grouped by category, with checkboxes and protections. |

### Explorer keys

`⏎` open · `⌫` parent · `space` mark · `d` delete marked (or current) · `x` clear marks ·
`T` heaviest files · `r`/`R` rescan current/selected · `a` allocated ↔ apparent ·
`f` show/hide files · `+`/`-` rows shown · `o` reveal in Finder · `/` filter · `p` pause
background scanning · `?` help · `L` log · `:` command line · `q` quit.

### Developer commands (`:`)

`:goto <path>` · `:rescan` · `:expand` (force a dense directory) · `:limit <n>` ·
`:top [n]` · `:filter <text>` · `:threads <n>` · `:dense <n>` · `:depth <n>` ·
`:pause` / `:resume` · `:mode allocated|apparent` · `:base decimal|binary` ·
`:trash on|off` · `:export <file>` (TSV of the current rows) · `:reveal` · `:mounts` ·
`:stats` · `:log` · `:theme [name]` · `:settings` · `:config [save|edit|reload]` · `:q`.

## How the scan stays fast

* **Focus first.** The queue has three lanes: interactive requests, everything below the
  directory you are viewing, everything else. Changing directory re-partitions the queue,
  so the focused branch gets every thread immediately; the rest keeps filling in.
* **Depth first.** The directory lanes are stacks. One branch is followed to its leaves
  before the next starts, which keeps the frontier small and settles sizes quickly.
* **Nothing per file.** Files are never nodes. A directory keeps aggregates; the files of
  the directory you are looking at are listed on demand (largest N kept in a bounded heap).
* **Dense directories.** A directory with more entries than `dense_directory_threshold`
  (20 000) is summed in aggregate and never listed: a folder with a million photos costs
  one pass and no memory. `:expand` overrides it deliberately.
* **Lazy depth.** Only `materialize_depth` levels below the scan origin become nodes;
  deeper directories are summed into their nearest ancestor. Entering such an ancestor
  starts a scan rooted there, so the tree never holds more than you can browse.
* **Iterative everywhere.** No recursion; an absolute `max_depth` guards against loops
  and pathological trees. Symlinks are never followed, hard links are counted once,
  other volumes are boundaries (shown, entered on demand).
* **Live aggregation.** Workers emit events; the interface thread applies them and sizes
  propagate up the tree as they arrive. The scanner never blocks the interface.
* **Measured, not assumed.** On macOS/APFS the standard library reader (`lstat`) beat a
  raw `fstatat` reader by about 1.5x and 4–8 threads beat 22, so those are the defaults.
  `--reader rustix` and `-j` let you check on your own machine; `tree-cleaner measure`
  prints the numbers.

Sizes are **allocated** bytes by default (what actually fills the volume, like `du`),
in decimal units like Finder; `a` toggles apparent sizes, `:base binary` switches to GiB.

## Deleting and cleaning

* Marked items are deleted permanently (`rm -rf` semantics) after you type `yes`, or moved
  to the Trash with `:trash on`.
* Every deletion and cleaning run opens a **progress modal** that locks the rest of the
  interface until it is done: a step bar over the items, bytes and entries freed so far,
  the item and file being removed right now, and the outcome of the last items. Nothing
  else can be typed while it runs, so no navigation, mark or rescan can race with the
  removal. `Esc` cancels after the current item finishes (items already removed stay
  removed; a directory removed half way is measured again). Ctrl+C still aborts the
  program. Removal walks each tree itself, file by file, which is what makes the live
  counters possible.
* Anything the current user cannot remove is collected; you are asked once whether to run
  `sudo` for those paths. The terminal is handed back so `sudo` asks for your password
  itself: the program never sees it.
* A list of critical locations (`/`, `/System`, `/usr`, your home, `~/Library`, ...) is
  refused no matter what. Add your own in `deletion.never_delete`.
* The cleaner finds: Trash, user and system caches, logs, Homebrew cache, pip/uv/npm/
  yarn/Cargo/Go/Gradle/Maven caches, Xcode DerivedData, archives, device support,
  simulator caches, Python virtual environments (`pyvenv.cfg` present), `node_modules`,
  Cargo `target/` (next to a `Cargo.toml`), build artifacts, Docker images, stopped
  containers, dangling volumes and build cache.
* Protect what must stay with globs in `cleaner.protected_paths` (paths) and
  `cleaner.docker.protected_images` (`repo:tag`, `repo` or id). Protected items are shown
  but cannot be selected. Images used by any container are protected automatically.

## Configuration

`~/.config/tree-cleaner/config.toml` (or `$XDG_CONFIG_HOME/tree-cleaner/config.toml`). Every
key is optional; see [`config.example.toml`](config.example.toml) for the full set with
comments.

The **Settings** popup (`:settings`) edits everything live on top of the current screen:
theme, size mode and units, rows shown, scanner threads and thresholds, deletion mode,
Docker options. Changes apply immediately; `s` writes them to the file (`:config save` does
the same from any screen), `e` opens the file in `$VISUAL`/`$EDITOR` and reloads it, `R`
resets to the defaults, `Esc` closes. Quitting with unsaved preference changes asks whether
to save them.

### Themes

Eleven built-in palettes: `claude` (default), `btop`, `nord`, `dracula`, `gruvbox`,
`catppuccin`, `tokyo-night`, `solarized`, `monochrome`, `light` (for light terminal
backgrounds) and `basic` (16 ANSI colours, also forced automatically when the terminal has
no truecolor). Switch with `←`/`→` on the Theme row of the Settings popup or with
`:theme <name>`; `:theme` alone lists them.

Any colour can be overridden in a `[theme]` table of the config file, on top of the chosen
palette. Values are `#rrggbb`, `#rgb`, an ANSI name (`red`, `light_blue`, `dark_gray`, ...)
or a 0-255 palette index:

```toml
[view]
theme = "nord"

[theme]
accent = "#d08770"        # highlights, keys, selected bars
directory = "#81a1c1"     # directory names
success = "#a3be8c"
warning = "#ebcb8b"
danger = "#bf616a"
selection_background = "#3b4252"
```

The full list of keys: `accent`, `accent_soft`, `directory`, `file`, `text`, `muted`,
`faint`, `success`, `warning`, `danger`, `border`, `border_focused`,
`selection_background`, `bar_track`.

## Architecture

Domain-driven, four layers, dependencies pointing inwards:

```
src/
├── domain/          pure model, no I/O
│   ├── storage/     ByteSize, FileTree (arena of directories), scan events, policy, volumes
│   ├── cleaning/    categories, candidates, protection rules, plans and reports
│   ├── deletion/    plans, reports, critical path guard
│   └── ports/       traits: DirectoryReader, VolumeProvider, FileRemover,
│                    PrivilegeEscalator, ContainerEngine, CommandRunner, FileRevealer
├── application/     use cases
│   ├── scanning/    ScanEngine (workers), TaskQueue (focus lanes), ScanWorker,
│   │                TreeCoordinator (owns the tree, applies events, answers the UI)
│   ├── cleaning/    discovery rules, known cache locations, CleaningService
│   ├── deletion/    DeletionService
│   └── configuration/
├── infrastructure/  adapters: std and rustix readers, mount table, sysinfo volumes,
│                    file remover (+Trash), sudo, Docker CLI, TOML store, shell
└── presentation/    ratatui interface: app state, screens, overlays, command line
```

## Development

```bash
cargo build --release
cargo clippy --all-targets -- -D warnings
./target/release/tree-cleaner measure ~/Downloads --json
```

`tree-cleaner measure` is the fastest way to validate the engine against `du -sk`.

## Releases

Versioning is automatic ([semantic-release](https://semantic-release.gitbook.io/), config in
`.releaserc.json`). Every push to `main` runs the checks and then decides from the
[Conventional Commits](https://www.conventionalcommits.org/) since the last tag whether to
release:

| Commit | Release |
| --- | --- |
| `fix: …`, `perf: …`, `revert: …` | patch (0.2.0 → 0.2.1) |
| `feat: …` | minor (0.2.0 → 0.3.0) |
| `feat!: …` or a `BREAKING CHANGE:` footer | major (0.2.0 → 1.0.0) |
| `docs:`, `refactor:`, `chore:`, `ci:`, `build:`, `style:`, `test:` | none |

A release bumps `Cargo.toml` and `Cargo.lock`, writes `CHANGELOG.md`, points the Homebrew
formula at the new tag, commits all of that as `chore(release): vX.Y.Z [skip ci]`, tags
`vX.Y.Z`, publishes the GitHub release with notes, and attaches prebuilt binaries
(`scripts/release/*.sh` are the hooks; `.github/workflows/ci.yml` is the pipeline). Squash
merges keep the PR title, so title pull requests with the same prefixes. Never edit the
version by hand.

Pull requests also run the release in dry-run mode against their own branch, so a broken
preset or changelog template fails there instead of after the merge. The changelog preset
(`conventional-changelog-conventionalcommits`) has to stay on the major that matches the
`conventional-changelog-writer` semantic-release ships: the `9.x` series for
semantic-release 25. Version `10` moved to a new template engine that needs `writer@9`,
which fails at the point where the notes are rendered.

The Homebrew formula lives in [lf-bf/homebrew-tap](https://github.com/lf-bf/homebrew-tap)
and is generated there from the release checksums, so it always installs the newest
published binaries and no checksum is written by hand. That repository checks for a new
release hourly; `gh workflow run sync --repo lf-bf/homebrew-tap` picks one up immediately.

## License

MIT
