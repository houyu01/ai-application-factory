# AI Application Factory Engineering Rules

These rules apply to every change in this repository. They are intentionally
tool-agnostic so Claude Code, Codex, Trae, and other IDE agents can follow the
same contract.

## Active product target: Tauri desktop

- The active product is the local-first Tauri desktop application. Its
  implementation boundary is `frontend/` (TypeScript UI) plus `src-tauri/`
  (Rust commands, services, repositories, durable tasks, and providers).
- Implement every new feature, product fix, UI change, generated code, and
  refactor in the Tauri implementation. Do not introduce a separate local
  backend runtime.

## Product compatibility

- Preserve existing product behavior, API paths, response shapes, task states,
  database table names, and user-visible labels unless the request explicitly
  changes a product requirement.
- Treat existing tests and persisted SQLite data as compatibility contracts.
- Prefer small, reversible refactors. Add regression coverage before changing a
  persistence or task-processing boundary.

## File and module size

- Keep every hand-maintained source, test, configuration, and documentation file
  at or below 450 lines.
- Split a module before it reaches the limit. Use focused modules, facades, and
  domain-specific services instead of compressed code or generated duplication.
- Generated lockfiles and binary/media artifacts are exempt from this line
  limit, but source files that generate them are not.

## Documentation and change history

- AI agents may create and modify implementation code within the product
  boundaries defined above.
- Do not create, edit, or delete the root `README.md` unless the user
  explicitly requests that file to be changed.
- If a change history entry is needed, create it in the root `changlogs/`
  directory. Every entry must be a newly created Markdown file named
  `YYYY-MM-DD-HHmm-<topic>.md`; never append to or overwrite an existing entry.
- Do not add change history to the root `README.md`.

## Rust documentation requirements

- Every public Rust service, repository, and durable worker type must document
  the business flow that calls it, the problem it solves, and the boundary it
  owns.
- Every Tauri command or IPC route must document the frontend behavior that
  triggers it and what it changes or returns.
- Every persisted data model must document the asset represented by the table.
- Every persisted field must have a nearby comment describing its meaning,
  when it is read, and when it is changed.
- Keep comments close to the code they describe; do not maintain a separate
  undocumented schema that can drift from the models.

## Persistence

- Rust repositories own SQLite transactions and translate persisted rows into
  API dictionaries; application services must not open database connections.
- Keep SQL isolated in focused repository or documented one-time migration
  modules. Preserve existing table names and JSON compatibility contracts.
- Keep JSON-shaped product fields serialized through typed helpers, not
  ad-hoc string manipulation.

## Command and service boundaries

- Tauri commands and the local IPC router validate input and delegate to
  application services or gateways.
- Gateways compose use cases; services coordinate business rules and tasks;
  repositories persist data; provider clients call external services.
- Long-running generation must be represented by a durable database task before
  work starts, so refreshes and process restarts can recover the task.
- A refactor must preserve task status transitions and idempotent enqueue
  behavior.

## Verification

- Run frontend type-checking, Rust formatting and compilation checks, relevant
  Rust/frontend tests, and `git diff --check` for relevant changes.
- Check source line counts and inspect the diff before handing off work.
- Never commit secrets, local `.env` files, SQLite databases, media output, or
  generated build artifacts.
