# AI Application Factory Engineering Rules

These rules apply to every change in this repository. They are intentionally
tool-agnostic so Claude Code, Codex, Trae, and other IDE agents can follow the
same contract.

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

## Documentation requirements

- Every server-side class must have a docstring explaining which business flow
  calls it, the problem it solves, and the boundary it owns.
- Every HTTP endpoint must have a docstring explaining the frontend behavior
  that triggers it and what the endpoint changes or returns.
- Every ORM table model must document the asset represented by the table.
- Every ORM column must have a comment describing its meaning, when it is read,
  and when it is changed.
- Keep comments close to the code they describe; do not maintain a separate
  undocumented schema that can drift from the models.

## Persistence and ORM

- Use SQLAlchemy 2.x declarative ORM models for all application tables and
  database operations.
- Each table must have one corresponding model in
  `backend/src/infrastructure/orm_models/`.
- Repositories own transactions and translate ORM objects into API dictionaries;
  application services must not open database connections.
- Do not add raw SQL strings to application or repository code. Raw SQL is only
  allowed in an explicitly isolated, documented, one-time migration module.
- Keep JSON-shaped product fields serialized through a typed repository helper,
  not through ad-hoc SQL expressions.

## API and service boundaries

- Routers validate HTTP input and delegate to application services or gateways.
- Gateways compose use cases; services coordinate business rules and tasks;
  repositories persist data; LLM clients call providers.
- Long-running generation must be represented by a durable database task before
  work starts, so refreshes and process restarts can recover the task.
- A refactor must preserve task status transitions and idempotent enqueue
  behavior.

## Verification

- Run backend tests, frontend type-checking, compilation checks, and
  `git diff --check` for relevant changes.
- Check source line counts and inspect the diff before handing off work.
- Never commit secrets, local `.env` files, SQLite databases, media output, or
  generated build artifacts.
