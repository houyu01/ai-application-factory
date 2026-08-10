# Python-to-Rust Migration Audit

This is the acceptance record for the local-first desktop migration. Python is no longer a runtime dependency: Tauri invokes Rust, Rust owns SQLite and local media, and only configured model/storage providers leave the device.

## Completed contracts

| Python capability | Rust counterpart | Evidence |
| --- | --- | --- |
| HTTP router and query defaults | `src-tauri/src/api.rs` IPC router | All project, settings, game, task, cover, placeholder and history routes are dispatched locally; task polling defaults to `生成中`. |
| SQLite tables, legacy columns, task state | `db.rs`, `migration.rs`, `repository/` | Same table names and JSON fields; upgrades add historic columns before access. |
| Project library and selected-shot editor projection | `repository/project_list.rs`, `projects.rs` | Library hides screenplay/full task content and reports persisted queue position/state; editor bounds non-selected shots. |
| Durable generation queues and retry | `worker.rs`, `worker_queues.rs`, `repository/tasks.rs` | Separate language/image/video/audio queues, SQLite leases, idempotent enqueue, durable video polling, and expansion retry checkpoints. |
| Long-form screenplay expansion | `worker_expansion.rs`, `worker_long_plan.rs`, `skills.rs` | Story bible, optional four-topic web research, five-episode checkpoints, continuation, cancellation, validation, and long-form shot planning. |
| Prompt/quality workflows | `worker_text.rs`, `worker_prompt_helpers.rs`, `skills.rs` | All Python drama skill instructions are embedded in Rust; template provenance, rich references, structured fields, and quality rules persist locally. |
| Asset, variant, batch, placeholder, cover | `worker_batch.rs`, `worker_placeholder.rs`, `worker_cover.rs` | Python-equivalent reference checks, five-at-a-time batch state, image history, placeholder idempotency, and cover output count are durable. |
| Video/provider operations | `providers_video*.rs`, `service_cancellation.rs` | Ark, DashScope, Tencent request/poll/cancel semantics and reference-marker mapping are retained. |
| Model/storage configuration | `providers_probe.rs`, `settings.rs`, `storage.rs` | Probe-before-save, masked credentials, independent queue concurrency, and local/TOS/COS/OSS storage remain available. |
| Interactive-game graph and runtime | `repository/games.rs`, `worker_game.rs`, `planner.rs` | The original deterministic graph and node-video placeholder result remain local; no new game video-model call was introduced. |

## Prompt and model-call traceability

The former Python instructions are now runtime Rust data in `src-tauri/src/skills.rs`. Long-form expansion, shot prompts, quality checks, and model requests compose those instructions before calling a configured provider. If the required provider configuration is absent, Rust returns the same task error/fallback behavior as the corresponding Python flow; it does not pretend to have completed a remote generation.

## Verification completed

- `cargo fmt`, `cargo check`, and `cargo test` pass.
- The Rust suite covers local media, task idempotency, legacy SQLite columns, provider request contracts, prompt/template provenance, game sessions, selected-shot projection, project-card queue state, and input bounds.
- Every handwritten Rust source file is at or below 450 lines, and `git diff --check` passes.

## Release verification still required

Live provider probes deliberately require the user's real credentials and are not simulated by the regression suite. Before a signed public release, test each configured provider once, then build and inspect the macOS app/DMG.

The legacy Python database uses the same SQLite table names and is upgraded when opened by the Rust database layer. The desktop app stores new data in its macOS application-support directory; an existing Python database is not silently copied from an arbitrary source path, so importing a user's previous database must be an explicit, user-directed migration step.
