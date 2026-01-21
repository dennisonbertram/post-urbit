# Post-Urbit Frontend Implementation Prompt

Use this prompt with an LLM to build the Post-Urbit frontend.

---

## Prompt

```
# Post-Urbit Frontend Implementation

You are building the frontend for Post-Urbit, a Tauri 2.x-based personal computing
platform with a **System 7 Macintosh aesthetic**. The complete specifications are
in `docs/specs/`.

## READ THESE FILES FIRST (in order)

1. `docs/specs/10-VISUAL_DESIGN.md` - **System 7 design language** (critical!)
2. `docs/specs/00-PLANNING_DOMAINS.md` - Overview of all domains
3. `docs/specs/01-SHELL_ARCHITECTURE.md` - Shell structure, state management
4. `docs/specs/02-APP_SANDBOX_ISOLATION.md` - Multi-webview, postapp:// protocol
5. `docs/specs/07-SDK_DEVELOPER_EXPERIENCE.md` - TypeScript SDK structure
6. `docs/PLANNING_LOOP_LOG.md` - System reviews with known issues

## Design Direction

**Target Aesthetic**: Apple System 7 (1991-1997) with full color support

Key visual elements:
- Chicago-style bitmap fonts (12px)
- 3D beveled buttons and controls
- Window chrome with title bar stripes, close/zoom boxes
- Grayscale chrome, color content
- 32x32 pixel art icons with 1px black outlines
- Inset/outset bevels for depth
- Classic alert dialogs with Stop/Caution/Note icons
- Menu bar at top with dropdown menus

DO NOT use:
- Modern flat design
- Rounded corners (except buttons)
- Smooth gradients
- Anti-aliased small text
- macOS Aqua or Big Sur styling

## Architecture Summary

- **Tauri 2.x** with multi-webview (one webview per sandboxed app)
- **Shell**: React + TypeScript + Zustand + custom System 7 components
- **Apps**: Isolated webviews served via `postapp://{app_id}/` custom protocol
- **IPC**: Single bridge command `postbridge_invoke` with CBOR encoding
- **State**: Zustand slices for apps, windows, permissions, resources

## Key Constraints

- Apps have NO access to Tauri APIs (only `postbridge_invoke`)
- Shell is the only privileged webview
- All permission prompts rendered by shell (anti-spoofing)
- Session tokens injected via `window.__POSTURBIT_BOOTSTRAP__`

## Implementation Order

1. **Phase 0**: Run gating spikes (`PHASE_0_GATING_SPIKES.md`)
2. **Design system**: Build System 7 component library first
   - Window chrome component
   - Button, checkbox, radio, input components
   - Menu bar and dropdown components
   - Alert/dialog components
   - Icon system
3. **Shell scaffold**: Tauri project + React shell + Zustand stores
4. **Custom protocol**: `postapp://` handler in Rust
5. **Bridge**: `postbridge_invoke` command + CBOR codec
6. **Multi-webview**: App webview creation/management
7. **SDK**: `@posturbit/sdk` TypeScript package
8. **Permission UI**: Shell-rendered System 7 style prompts
9. **App lifecycle**: Install, launch, close, eviction flows

## Known Issues (fix as you go)

- Use versioned method names (`storage.v1.get` not `storage.get`)
- Shell commands use `shell_*` prefix (not `app_*`)
- SDK envelope fields at top level (not in params)
- Events to apps via `events.*` long-poll (not push)

## Output Structure

packages/
├── shell/                    # Tauri + React shell application
│   ├── src-tauri/           # Rust backend
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── bridge.rs    # postbridge_invoke handler
│   │   │   ├── protocol.rs  # postapp:// custom protocol
│   │   │   ├── webview.rs   # Multi-webview management
│   │   │   └── session.rs   # Session/token management
│   │   └── Cargo.toml
│   └── src/                  # React frontend
│       ├── components/
│       │   ├── system7/     # System 7 design components
│       │   │   ├── Window.tsx
│       │   │   ├── Button.tsx
│       │   │   ├── MenuBar.tsx
│       │   │   ├── Alert.tsx
│       │   │   └── ...
│       │   └── shell/       # Shell-specific components
│       │       ├── AppGrid.tsx
│       │       ├── PermissionPrompt.tsx
│       │       └── StatusBar.tsx
│       ├── stores/          # Zustand stores
│       │   ├── apps.ts
│       │   ├── windows.ts
│       │   ├── permissions.ts
│       │   └── resources.ts
│       ├── App.tsx
│       └── main.tsx
├── sdk/                      # @posturbit/sdk TypeScript package
│   ├── src/
│   │   ├── client.ts
│   │   ├── transport.ts
│   │   ├── codec.ts
│   │   └── namespaces/
│   └── package.json
└── system7-ui/              # Optional: Standalone System 7 component library
    ├── src/
    └── package.json

## Fonts to Use

- **ChicagoFLF** - Free recreation of Chicago font (npm: chicago-flf)
- **Geneva** - Or similar sans-serif fallback
- CSS: `font-family: "ChicagoFLF", "Geneva", system-ui, sans-serif`

## Color Palette

/* System Chrome */
--color-white: #FFFFFF;
--color-light-gray: #EEEEEE;
--color-desktop: #DDDDDD;
--color-dark-gray: #888888;
--color-black: #000000;

/* Accents */
--color-highlight: #000080;
--color-selection: #0000CC;

Begin by reading the spec files, then build the System 7 component library before
implementing the shell functionality.
```

---

## Quick Reference

| Spec | Purpose |
|------|---------|
| `10-VISUAL_DESIGN.md` | System 7 aesthetic, colors, typography, components |
| `01-SHELL_ARCHITECTURE.md` | State management, component structure |
| `02-APP_SANDBOX_ISOLATION.md` | Multi-webview, security model |
| `04-SECURE_BRIDGE_PROTOCOL.md` | IPC protocol, CBOR encoding |
| `06-PERMISSION_SYSTEM.md` | Permission prompts, TOCTOU flow |
| `07-SDK_DEVELOPER_EXPERIENCE.md` | TypeScript SDK for apps |
| `08-APP_LIFECYCLE_MANAGEMENT.md` | Install, launch, close flows |

## Inspiration Links

- [GUIdebook - System 7 Screenshots](https://guidebookgallery.org/screenshots/macos70)
- [Macintosh Human Interface Guidelines 1992](https://archive.org/details/apple-human-interface-guidelines-1992)
- [Susan Kare Icons](https://kare.com/)
- [7.css - Windows 7 CSS](https://khang-nd.github.io/7.css/) (adapt for Mac)
- [98.css](https://jdan.github.io/98.css/) (reference implementation)
