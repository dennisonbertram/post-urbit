# ADR-002: Component Library Constraints

## Status
Accepted

## Context
The shell needs a consistent, accessible, and secure UI component system. Given the shell is the privileged surface, components must:
- Never execute arbitrary HTML/JS from app manifests
- Support full keyboard navigation
- Meet WCAG 2.1 AA accessibility standards
- Work with the theming system
- Be performant for complex layouts

## Decision
Use **shadcn/ui** with **Tailwind CSS** and **Radix UI** primitives.

### Rationale
1. shadcn/ui provides copy-paste components (not a dependency)
2. Built on Radix UI (excellent accessibility by default)
3. Tailwind enables design-token-based theming via CSS variables
4. Components are fully customizable (we own the code)
5. No runtime injection of external styles

### Constraints
1. **No dangerouslySetInnerHTML** in shell components
2. **All user/app content** rendered via text nodes only
3. **No SVG icons from app manifests** (use predefined icon set - Lucide)
4. **No dynamic CSS/style injection** from external sources
5. **Radix primitives** for all interactive elements (Dialog, Menu, Tooltip)

## Options Considered

### Option 1: shadcn/ui + Tailwind (CHOSEN)
**Pros:**
- Components are owned code (not npm dependency)
- Radix UI accessibility built-in
- Tailwind utility classes compile to static CSS
- CSS variable theming works out of box
- Large community, many examples

**Cons:**
- Need to keep components updated manually
- Tailwind config required

### Option 2: Chakra UI
**Pros:**
- Good accessibility
- Built-in dark mode
- Comprehensive component set

**Cons:**
- Runtime CSS-in-JS (emotion)
- Heavier bundle
- Less customizable without ejecting

### Option 3: MUI (Material UI)
**Pros:**
- Comprehensive enterprise-grade library
- Strong accessibility

**Cons:**
- Material Design aesthetic (not neutral)
- Heavy bundle size
- Runtime style injection

### Option 4: Custom Components
**Pros:**
- Full control
- Minimal dependencies

**Cons:**
- Significant development time
- Must implement all accessibility
- Maintenance burden

## Consequences

### Security Implications
- Shell components never render app-provided HTML
- All text content escaped automatically by React
- No `eval()` or `new Function()` in component code
- Icon set is predefined (Lucide icons), no external SVG

### Performance Implications
- Tailwind produces small, static CSS
- Radix primitives are tree-shakeable
- No runtime CSS generation overhead

### Developer Experience
- Familiar Tailwind patterns
- Component code is local and reviewable
- TypeScript types for all components

## Rollback Plan
Switching component libraries would require:
1. Replacing all shadcn components with new library
2. Updating Tailwind theme tokens to new format
3. Significant refactoring effort

## Related
- [Domain 1: Shell Architecture](../specs/01-SHELL_ARCHITECTURE.md)
- [ADR-001: State Management](./ADR-001-state-management.md)
