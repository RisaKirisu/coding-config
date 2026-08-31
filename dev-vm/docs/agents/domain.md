# Domain Docs

How engineering skills should consume this repo's domain documentation.

## Before exploring, read these

- `CONTEXT.md` at the repository root
- `CONTEXT-MAP.md` if it exists
- Relevant ADRs under `docs/adr/`

If these files do not exist, proceed silently. `/domain-modeling`, `/grill-with-docs`, and `/improve-codebase-architecture` create them lazily when terms or decisions are resolved.

## File structure

This is a single-context repository:

```text
/
├── CONTEXT.md
├── docs/adr/
└── src/
```

## Use the glossary's vocabulary

Use domain terms as defined in `CONTEXT.md`. Do not drift to synonyms it explicitly avoids.

If a needed concept is absent, reconsider whether the term belongs or note the gap for `/domain-modeling`.

## Flag ADR conflicts

If proposed work contradicts an ADR, surface the conflict explicitly rather than silently overriding it.
