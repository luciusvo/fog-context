# ADR 001: Stale Graph Checks in Read Tools

## Context
Read tools (lookup, trace, outline, domains, gaps) need to warn users if the AST is stale.

## Decision
Inject stale warnings via Composition (Level 2) into the output of read tools by updating their `handle` signatures to accept `project_root` and calling `stale::check_stale` before matching results.

## Consequences
Read tools now surface freshness awareness, directing users to run `fog_scan`.
