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

```bash
# from the repository
cargo install --path .

# Homebrew (tap from the repository; a tagged release makes it a normal bottle later)
brew tap lf-bf/tree-cleaner git@github.com:lf-bf/tree-cleaner.git
brew install --HEAD lf-bf/tree-cleaner/tree-cleaner
```

Requires Rust 1.85 or newer to build. Runs on macOS and Linux, in any terminal
(truecolor when available, 16 colours otherwise, `NO_COLOR` respected).

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
`:stats` · `:log` · `:config [save]` · `:q`.

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

`~/.config/tree-cleaner/config.toml` (or `$XDG_CONFIG_HOME`). Every key is optional; see
[`config.example.toml`](config.example.toml) for the full set with comments.
`:config save` writes the settings you changed at runtime.

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
cargo clippy --all-targets
./target/release/tree-cleaner measure ~/Downloads --json
```

`tree-cleaner measure` is the fastest way to validate the engine against `du -sk`.

## License

MIT
