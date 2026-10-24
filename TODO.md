# Linux / Apple Music work plan

## Working agreement

- Implement independent fixes and features incrementally, with one coherent commit per completed item. Keep this document updated with evidence, decisions, verification results and blockers in the same commit as the work.
- Use repository-local Git author `gpt-astra <gpt-astra@users.invalid>` (an automation label, not a GitHub account). Do not change global Git identity. Preserve attribution when adapting upstream work.
- Do not add permanent test code. Remove temporary validation code before committing; retain existing tests. Do not claim a check passed unless it ran successfully.
- Every application/runtime test must finish within 10 seconds, including cleanup of child processes. Before opening a login webview, establish an enforced memory limit covering its child processes. Do not run long crash reproductions.
- Reproduce or establish a concrete failure path before fixing uncertain bugs. Add defensive code only for demonstrated risks; document the evidence and use the smallest necessary change. No speculative retries or silent fallbacks.
- Reuse existing components and preserve project style and dependency direction. Consider layout invalidation, rendering cost, network concurrency and audio latency.
- Defer work requiring music-account login. Never record credentials in this document, logs or commits.
- GitHub authentication is separate from Git author identity. Fork/push/PR work requires valid user-authorized authentication; do not create or impersonate a GitHub account.

## Baseline and environment

- Source baseline: `e2d099c3` (`chore(nix): point the flake at 0.44.1`); working tree was clean before creating this plan.
- Local inspection: Fedora Linux 44 KDE Plasma Desktop Edition, Wayland, Intel Raptor Lake-P / Iris Xe graphics.
- Host WebKitGTK: `webkit2gtk4.1-2.54.1-1.fc44.x86_64`. This falls within the range targeted by PR #925, but does not establish the version loaded by Flatpak.
- Installed Sonora: user Flatpak 0.44.1, remote `sonora`, runtime `org.gnome.Platform/x86_64/50`, application commit `c86e7b0231a8085af39c56010f3c36083f331ba60674c461d583d0dea77cb4e1`.
- GitHub authentication has been renewed. Fork: `https://github.com/Augustus-S/sonora`; remote `fork` points there and `origin` remains upstream. PR publication is tracked below.
- Previous audit fetched 343 issues (including closed) and 723 PRs. Raw material is under `/tmp/sonora-audit/`; this is temporary, so durable conclusions and source links are recorded below. Issue/PR states are audit snapshots and must be rechecked before implementation.
- No Sonora application or login test has been run. Only static inspection and environment/CLI queries have been performed.

## Ordered work

Status: `pending` = not implemented; `investigate` = evidence required; `deferred` = requires an account or external capability. An unchecked item is not complete.

| ID | Status | Work and evidence | Acceptance / verification |
| --- | --- | --- | --- |
| L01 | implemented; login verification deferred | Address Apple Music login memory exhaustion using [PR #925](https://github.com/sonorahq/sonora/pull/925) as the starting point. Current `crates/webview/src/linux.rs` only disables DMA-BUF for NVIDIA. | Review upstream patch and attribution; determine actual runtime WebKit version; preserve explicit user overrides; compile/check the affected crate. Account login verification remains deferred. |
| L02 | investigate | Assess bounded login lifetime and failure/recovery UX after L01. The reported OOM is evidence for resource containment, not for arbitrary retries or unrelated guards. | Inspect existing close/cancellation/timeouts and manual-token entry before proposing additions. Specify any remaining failure path and use existing UI/state mechanisms. |
| L03 | investigate | Random Linux playback crash: [#926](https://github.com/sonorahq/sonora/issues/926). Fedora report [#971](https://github.com/sonorahq/sonora/issues/971) was closed as a duplicate, not as fixed. Review overlap with [PR #990](https://github.com/sonorahq/sonora/pull/990). | Inspect existing local crash records and relevant source without a long reproduction. Only implement a fix after locating a concrete defect; otherwise record the blocker. |
| L04 | implemented; profiling deferred | Volume-slider lag and broad layout invalidation: [#869](https://github.com/sonorahq/sonora/issues/869). Relevant path: playback volume -> settings notification -> observers. | Use bounded profiling if safe; isolate unnecessary invalidation while preserving live gain and persistence. Compare the same short interaction before/after. GPU activity alone is not proof of a bug. |
| L05 | investigate | KDE Wayland window restoration: [#919](https://github.com/sonorahq/sonora/issues/919). `crates/state/src/settings.rs` already saves window bounds. | Trace save and restore, distinguish normal/maximized size from compositor-controlled position. Fix only application-side defects; document unsupported positioning instead of promising it. |
| L06 | partial; webview diagnostics implemented | Fedora / Flatpak diagnostics, extending existing requirements reporting. | Inspect current UI and reuse it; show actual WebKit availability/version, session, audio backend and Widevine status without secrets or repeated expensive probes. Break into small changes after identifying API boundaries. |
| L07 | investigate | Native PipeWire backend: [#341](https://github.com/sonorahq/sonora/issues/341), existing TODO in `crates/music/Cargo.toml`. PulseAudio support already exists. | Verify pinned audio dependency support and supported distribution requirements before changing defaults. Preserve existing backend compatibility; bounded playback checks only. |
| L08 | pending | Linux middle-click paste regression verification: [#651](https://github.com/sonorahq/sonora/issues/651), [PR #1024](https://github.com/sonorahq/sonora/pull/1024). Current `crates/ui/src/input/` already implements it. | Verify existing behavior and GNOME preference handling as relevant. Do not implement a duplicate feature. |
| A01 | deferred | Apple Music date-added sorting: [#1042](https://github.com/sonorahq/sonora/issues/1042), existing [PR #1043](https://github.com/sonorahq/sonora/pull/1043). | Review existing patch/dependencies before adapting. Do not substitute album dates for track dates. Account-backed ordering verification waits for login. |
| A02 | deferred | Recently added library and additional personalized station entries: [#1061](https://github.com/sonorahq/sonora/issues/1061). Recommendations, recently played and track-seeded stations already exist. | Confirm available provider data, then add only missing navigation/content. Split recently added and station browsing into separate changes. |
| A03 | deferred | Uploaded / matched iTunes cloud library: [#955](https://github.com/sonorahq/sonora/issues/955). | First establish enumeration and authorized playback with an account containing these items. Keep cloud tracks distinct from catalog-only identifiers. Do not mark complete with listing alone. |
| A04 | deferred | Google Cast with Apple Music: [#995](https://github.com/sonorahq/sonora/issues/995). | Establish authorized receiver playback feasibility before implementation. Then separate discovery, connection/control and queue integration. Discovery alone is not Apple Music Cast support; requires a receiver and account verification. |
| G01 | in progress | GitHub fork and topic PRs. | User renews CLI authentication; verify it without displaying credentials, create a fork under the authenticated account, retain upstream remote, and submit independently reviewable topic PRs. Do not force-push shared branches. |

## Capability boundaries (not promised features)

- Current Apple Music stream selection is AAC 256 kbps / HE-AAC 64 kbps (`crates/music/src/apple/stream.rs`). Lossless support has not been established.
- Downloads still require online checks; true offline playback has not been established.
- The web-derived token lacks permission for Apple's official lyrics API (`crates/music/src/apple/mod.rs`). Third-party lyrics are a separate capability.
- Do not treat macOS Bluetooth report #975 or Windows crackling report #893 as confirmed Fedora bugs.

## Completion checklist for each implementation

- [ ] Record concrete evidence and the smallest chosen change under the relevant ID.
- [ ] Run targeted formatting/build/static checks and applicable bounded runtime checks; record failures and skipped checks explicitly.
- [ ] Review performance effects, compatibility and diff; remove any temporary test code.
- [ ] Update this document and add user-observable changes to `CHANGELOG.md`.
- [ ] Commit the coherent change with a conventional, lowercase, single-line message.
- [ ] If authenticated, prepare the corresponding topic PR without unrelated changes.

## Progress log

### Planning baseline

- Queried local OS, session, GPU, host WebKitGTK and installed Flatpak without launching Sonora.
- `gh auth status` failed: stored credential is invalid. Authentication requires user action in their terminal; no credential was printed or modified.
- Created this tracked plan. No application fixes or new features were complete at that point.

### L01 — WebKitGTK renderer workaround

- Adapted the version-based DMA-BUF workaround from SKetU-l's PR #925, commit `fbd555428395c9d29483aad58855cc77c2ea4909` (still open and unmerged when rechecked). Existing NVIDIA handling and explicit environment overrides remain intact. No new retry or fallback layer was added.
- Read the Flatpak runtime library's exported version functions with `objdump`, without loading it or opening a browser: major 2, minor 54. The host package is 2.54.1. Both are in the affected range.
- `cargo +stable fmt --all -- --check` and `git diff --check` passed. Workspace `cargo check --locked --offline -p webview` could not resolve the uncached pinned librespot Git dependency.
- To isolate the edited crate, copied its sources to a temporary standalone workspace using cached anyhow 1.0.104, log 0.4.34 and libc 0.2.190. `cargo +stable check --offline` and all four existing unit tests passed (each command under one second). This is not a locked whole-workspace build; no test source was added.
- Runtime Apple Music login verification remains deferred. No login window or Sonora instance was opened. Next round: validate the patched application under an enforced process-group memory/time limit after resolving workspace build prerequisites.

### L04 — Avoid settings-wide invalidation on volume changes

- Confirmed the concrete fan-out: `Playback::set_volume` calls `AppSettings::set_volume`, whose notification wakes Workspace, sidebars, settings-page measurement, ambient rendering and other preference observers on every drag update.
- Changed only volume persistence to use the existing `save_state_quietly` path. `Playback` still updates both engine gains and notifies live volume observers; the same 300 ms SQLite save debounce is retained.
- Audited volume readers and setters across `crates/`: only playback initialization reads `AppSettings::volume`; live UI, remote control and equalizer volume come from `Playback`. No additional cache, event type or defensive check was needed.
- `cargo +stable fmt --all -- --check` and `git diff --check` passed. Full state-crate compilation and before/after GUI profiling remain blocked by uncached workspace Git dependencies. An online check spent its 8-second budget fetching dependencies and was terminated, not passed. The change removes proven redundant notifications, but does not claim all lag in #869 is resolved.

### L06 — Sign-in diagnostics (first slice)

- Added the actual loaded WebKitGTK version to the GTK thread's startup diagnostics and enabled the existing `webview` debug target in the disk logger. The existing backend/renderer messages now reach the application log without changing console verbosity. No extra process, periodic probe or public API was introduced.
- Existing output selection already logs the audio backend, and the Widevine host logs module opening. A unified status UI and GPU summary remain for the next round; do not label this logging slice as a completed diagnostics panel.
- Validated systemd user-service containment first with `/usr/bin/true`: MemoryMax=384M, MemorySwapMax=0, RuntimeMaxSec=6s, TimeoutStopSec=1s, plus an 8-second outer timeout.
- Ran a temporary standalone blank-page example with those limits. It opened only `about:blank`, asserted the workaround variable was `1`, dropped the page and exited successfully in 1.499 seconds; peak memory 186.8 MiB. Output identified WebKitGTK 2.54.1, the automatic DMA-BUF workaround and GdkWaylandDisplay. No Apple URL, account or credential was used. This proves the host runtime path, not Apple login or the Flatpak binary.
- Temporary example source is removed after validation. The standalone crate uses newer cached libc/log versions than the repository lockfile; whole-workspace compilation remains outstanding.
