# ADR 002: Session Statistics in DbPool

## Context
We need to track tool call telemetry per session for each project.

## Decision
Store `call_counts` in `DbPool` (Level 4 Simple Class) to persist mutable state across tool requests. Router injects this state into `fog_brief`.

## Consequences
Agents can monitor invocation volume via `fog_brief`.
