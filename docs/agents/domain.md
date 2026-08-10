# Domain docs

This repository uses a multi-context domain-document layout because the Rust application and the `libs/` pnpm workspace have separate runtime and vocabulary boundaries.

## Before exploring

- Read `CONTEXT-MAP.md` at the repository root.
- Read the `CONTEXT.md` file for every context relevant to the task.
- Read system-wide ADRs in `docs/adr/` and context-specific ADRs below the relevant context when they exist.

## Contexts

- Rust/Dioxus application: `src/CONTEXT.md`
- JavaScript workspace libraries: `libs/CONTEXT.md`

Context files and their ADR directories are created lazily when domain terms or architectural decisions are resolved.

## Vocabulary

Use the vocabulary defined by the relevant context file in issue titles, designs, tests, and implementation notes. If a needed term is missing or overloaded, resolve it before building on it and record the decision in that context's documentation.
