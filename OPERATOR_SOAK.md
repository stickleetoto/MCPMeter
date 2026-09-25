# Operator Soak Lane

## Purpose

This branch is a real-product stress target for DevSeat Minimal Operator.

Unlike `devseat-operator-canary`, this lane should produce useful MCPMeter work while exercising:

- persistent worker reuse
- up to three implementation workers
- serialized integration
- batch verification
- repair-only recovery
- task identity recovery after restart
- Windows capsule persistence

## Isolation

```text
main
└─ operator/integration-b003
   ├─ worker/<issue-a>
   ├─ worker/<issue-b>
   └─ worker/<issue-c>
```

Do not promote `operator/integration-b003` to `main` until the desired soak checkpoint is explicitly reviewed.

## First checkpoint

Run B003-A as the first real soak batch.

Desired evidence:

- no duplicate Issue or dispatch
- no Worker-04
- worker chat reuse survives browser/Codex restart
- local capsule state saves without the prior Windows hang
- all three items remain unchecked until batch verification
- any verification failure produces repair-only work
- final verified integration commit is identifiable

## Existing repository state

- `docs/ROADMAP.md` remains the canonical product roadmap.
- Existing operator branches B001/B002 are historical and behind `main`.
- PR #21 is already open against `main` and is not part of B003 unless explicitly adopted later.
