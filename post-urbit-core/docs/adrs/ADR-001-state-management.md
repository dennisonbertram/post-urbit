# ADR-001: State Management Approach

## Status
Accepted

## Context
The Post-Urbit shell needs to manage UI state (theme, layout, focus), synchronize with authoritative Rust state (apps, sessions, permissions), and provide performant updates to a complex component tree with potentially 5-10 running apps.

Key requirements:
- Minimal boilerplate
- TypeScript-first
- Selective subscriptions (avoid rerender storms)
- Clear separation between UI state and Rust-authoritative state
- Testable stores

## Decision
Use **Zustand** with a slice-based architecture.

### Rationale
1. Zustand is lightweight (~1KB), TypeScript-first, and React-focused
2. Built-in `subscribeWithSelector` prevents rerender storms
3. Slices provide clear domain separation
4. No context provider tree required (simpler component tree)
5. Easy to test (stores are plain objects)
6. Already mentioned in existing architecture docs

### Architecture
- **UI Slice**: Pure frontend state (theme, sidebar, modals)
- **Mirrored Slices**: State that reflects Rust (apps, windows, connectivity)
- **Event Sync**: Tauri events push Rust state changes to Zustand

## Options Considered

### Option 1: Zustand (CHOSEN)
**Pros:**
- Minimal API surface
- No providers required
- Excellent TypeScript support
- subscribeWithSelector built-in
- Battle-tested in production apps

**Cons:**
- Less opinionated than Redux Toolkit
- No built-in devtools (requires middleware)

### Option 2: Redux Toolkit
**Pros:**
- Industry standard
- Excellent devtools
- Built-in async handling (RTK Query)

**Cons:**
- More boilerplate
- Heavier bundle size
- Provider required at root
- Overkill for shell-only state

### Option 3: Jotai/Recoil (Atomic)
**Pros:**
- Fine-grained reactivity
- Minimal rerenders by default

**Cons:**
- Requires provider
- Less suitable for domain slices
- Learning curve for team

### Option 4: React Context + useReducer
**Pros:**
- Built into React
- No dependencies

**Cons:**
- Rerender storms without careful memoization
- No built-in devtools
- More boilerplate for slices

## Consequences

### Security Implications
- State stores must not contain sensitive data (tokens stored in Rust only)
- Store snapshots in devtools must not leak secrets

### Performance Implications
- subscribeWithSelector prevents unnecessary rerenders
- Window list updates only affected components

### Developer Experience
- Simple API: `useShellStore(selector)`
- Clear pattern: UI state local, Rust state synced
- Easy unit testing of store logic

## Rollback Plan
Migration to Redux Toolkit is possible by:
1. Converting slices to Redux slices
2. Replacing `useShellStore` with `useSelector`
3. Adding Provider wrapper

## Related
- [Domain 1: Shell Architecture](../specs/01-SHELL_ARCHITECTURE.md)
- [TAURI_INTEGRATION_PLAN.md](../TAURI_INTEGRATION_PLAN.md)
