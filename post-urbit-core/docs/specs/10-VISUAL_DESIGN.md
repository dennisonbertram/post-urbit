# Domain 10: Visual Design Specification

## Status: Draft
## Version: 1.0.0
## Last Updated: 2026-01-20

---

## Design Direction

**Target Aesthetic**: Apple System 7 (1991-1997) with color support

The Post-Urbit shell recreates the warmth and clarity of classic Macintosh System 7, featuring beveled 3D controls, Chicago-style typography, and pixel-perfect window chrome. While the chrome remains faithful grayscale, app content and icons support full color.

### Reference Systems
- **Primary**: System 7.0 - 7.6 (1991-1996)
- **Secondary**: Mac OS 8.0 Platinum (for subtle refinements)
- **Avoid**: Mac OS X Aqua, modern flat design

---

## 1. Color Palette

### System Chrome (Grayscale)

```
┌─────────────────────────────────────────────────────┐
│  Name              │  Hex      │  Usage             │
├─────────────────────────────────────────────────────┤
│  White             │  #FFFFFF  │  Highlight, light  │
│  Light Gray        │  #EEEEEE  │  Window background │
│  Desktop Gray      │  #DDDDDD  │  Desktop pattern   │
│  Medium Gray       │  #AAAAAA  │  Inactive elements │
│  Dark Gray         │  #888888  │  Shadows, borders  │
│  Charcoal          │  #555555  │  Text shadows      │
│  Black             │  #000000  │  Text, outlines    │
└─────────────────────────────────────────────────────┘
```

### Accent Colors (System 7 Palette)

```
┌─────────────────────────────────────────────────────┐
│  Name              │  Hex      │  Usage             │
├─────────────────────────────────────────────────────┤
│  Selection Blue    │  #0000CC  │  Highlighted text  │
│  Highlight         │  #000080  │  Selected items    │
│  Alert Yellow      │  #FFCC00  │  Warning icons     │
│  Error Red         │  #DD0000  │  Error states      │
│  Success Green     │  #008800  │  Success states    │
│  Folder Blue       │  #6699CC  │  Folder icons      │
└─────────────────────────────────────────────────────┘
```

### Desktop Patterns

Support classic System 7 desktop patterns:
- Solid gray (#DDDDDD default)
- Classic diagonal lines
- Brick pattern
- User-selectable from control panel

---

## 2. Typography

### Font Stack

```css
/* Primary UI Font - Chicago-style */
--font-system: "ChicagoFLF", "Geneva", "Charcoal", system-ui, sans-serif;

/* Monospace - Monaco style */
--font-mono: "Monaco", "Geneva Mono", "Courier", monospace;

/* Icon labels */
--font-icon: "Geneva", "ChicagoFLF", sans-serif;
```

### Recommended Web Fonts
- **ChicagoFLF**: Free recreation of Chicago (primary)
- **Geneva**: System font for smaller text
- **Pixelated fonts**: For authentic 1-bit icon labels

### Type Scale

```
┌──────────────────────────────────────────────────┐
│  Element           │  Size   │  Weight  │  Font  │
├──────────────────────────────────────────────────┤
│  Menu bar          │  12px   │  normal  │  Chicago │
│  Window title      │  12px   │  bold    │  Chicago │
│  Body text         │  12px   │  normal  │  Geneva  │
│  Button label      │  12px   │  normal  │  Chicago │
│  Icon label        │  9px    │  normal  │  Geneva  │
│  Dialog text       │  12px   │  normal  │  Chicago │
│  Alert title       │  12px   │  bold    │  Chicago │
└──────────────────────────────────────────────────┘
```

### Text Rendering
- No anti-aliasing for small sizes (crisp pixel fonts)
- Optional: CSS `font-smooth: never` or `text-rendering: optimizeSpeed`
- High contrast: pure black (#000) on light gray (#EEE)

---

## 3. Window Chrome

### Standard Window

```
┌─────────────────────────────────────────────────────────────┐
│▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓│
│┌─┐═══════════════ Window Title ═══════════════════════┌─┐│
│└─┘                                                    └─┘│
├─────────────────────────────────────────────────────────────┤
│                                                             │
│                     Content Area                            │
│                     (Light Gray #EEEEEE)                    │
│                                                             │
│                                                         ┌─┐ │
│                                                         │▲│ │
│                                                         ├─┤ │
│                                                         │░│ │
│                                                         │░│ │
│                                                         ├─┤ │
│                                                         │▼│ │
│                                                         └─┘ │
├────────────────────────────────────────────────────┬────────┤
│◄│░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│►│ resize │
└────────────────────────────────────────────────────┴────────┘
```

### Window Elements

**Title Bar**:
- Height: 20px
- Horizontal lines pattern (stripes) when active
- Solid gray when inactive
- Title centered, Chicago 12px bold

**Close Box** (top-left):
- 13x13px square
- 1px black border
- Inset bevel when pressed

**Zoom Box** (top-right):
- 13x13px square with inner square
- Toggles window size

**Borders**:
- Outer: 1px black
- Inner: 3D bevel (white top/left, dark gray bottom/right)
- Content inset: 1px

**Resize Handle** (bottom-right):
- Diagonal lines pattern
- 15x15px hit area

### Window States

```css
/* Active Window */
.window.active {
  --title-bg: linear-gradient(#FFFFFF 1px, #000000 1px, #FFFFFF 2px, ...);
  --border-color: #000000;
  --shadow: 1px 1px 0 #555555;
}

/* Inactive Window */
.window.inactive {
  --title-bg: #DDDDDD;
  --title-color: #888888;
  --border-color: #888888;
}
```

---

## 4. Controls

### Buttons

**Standard Button**:
```
    ┌──────────────────────┐
   ╱│      Button Text     │
  ▕ │                      │╲
    └──────────────────────┘
     ╲▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁╱
```
- Rounded rectangle (2px radius)
- 3D bevel: white top/left, dark gray bottom/right
- Height: 20px
- Padding: 12px horizontal

**Default Button** (bold border):
```
    ╔══════════════════════╗
   ╱║      OK Button       ║
  ▕ ║                      ║╲
    ╚══════════════════════╝
     ╲▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁╱
```
- 3px black border
- Pulsing animation (optional)

**Pressed State**:
- Invert bevel (dark top/left, light bottom/right)
- Background darkens slightly
- Text shifts 1px down-right

### Checkboxes

```
 ☐  Unchecked       12x12px box, white fill, 1px border
 ☑  Checked         X mark or checkmark inside
 ▣  Mixed           Dash or gray fill
```

### Radio Buttons

```
 ○  Unselected      12x12px circle, white fill
 ◉  Selected        Filled center dot (6px)
```

### Text Fields

```
┌────────────────────────────────────┐
│ Text input here                    │
└────────────────────────────────────┘
```
- Inset bevel (dark top/left, light bottom/right)
- White background
- 1px black border
- 2px internal padding

### Popup Menus / Dropdowns

```
┌────────────────────────────┬───┐
│  Selected Item             │ ▼ │
└────────────────────────────┴───┘
         │
         ▼
    ┌────────────────────────────┐
    │  Option 1                  │
    ├────────────────────────────┤
    │▓▓Option 2 (selected)▓▓▓▓▓▓▓│
    ├────────────────────────────┤
    │  Option 3                  │
    └────────────────────────────┘
```
- Selection highlight: inverted (white text on black/blue)
- Shadow on dropdown: 1px offset

### Scrollbars

```
┌───┐
│ ▲ │  ← Arrow button (16x16)
├───┤
│░░░│  ← Track (gray pattern)
│░░░│
├───┤
│   │  ← Thumb (white, beveled)
│   │
├───┤
│░░░│
├───┤
│ ▼ │
└───┘
```
- Width: 16px
- Arrows: solid triangles
- Track: gray dither pattern
- Thumb: white with bevel

### Progress Bars

```
┌────────────────────────────────────┐
│▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░░░░░░│
└────────────────────────────────────┘
```
- Inset border
- Fill: solid or animated barber-pole stripes
- Indeterminate: candy-stripe animation

### Sliders

```
         ┌───┐
─────────┤   ├─────────
         └───┘
```
- Track: 2px inset line
- Thumb: 3D beveled rectangle or triangle

---

## 5. Icons

### Icon Sizes

```
┌─────────────────────────────────────────┐
│  Size      │  Usage                     │
├─────────────────────────────────────────┤
│  32x32     │  Desktop, Finder list      │
│  16x16     │  Menu bar, small lists     │
│  48x48     │  Large icon view (optional)│
└─────────────────────────────────────────┘
```

### Icon Style Guidelines

1. **Pixel-perfect**: Design at 1x, no anti-aliasing on edges
2. **Limited palette**: Use System 7 color palette
3. **Black outline**: 1px black border around shapes
4. **3D shading**: Light from top-left
5. **Drop shadow**: Optional 1px shadow bottom-right

### Core Icons Needed

**System**:
- Post-Urbit logo (app icon)
- Folder (open/closed)
- Document
- Trash (empty/full)
- Hard drive
- Network
- Preferences/Control Panel

**App States**:
- App icon (generic)
- App running indicator
- App installing
- App error

**Permissions**:
- Lock (locked/unlocked)
- Key
- Shield
- Warning triangle
- Question mark

**Actions**:
- Plus (add)
- Minus (remove)
- Checkmark
- X (close/cancel)
- Arrows (navigation)

### Icon Construction

```
32x32 App Icon Template:

  ░░██████████████████████████░░
  ░█▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓█░
  █▓▓╔════════════════════╗▓▓▓█
  █▓▓║                    ║▓▓▓█
  █▓▓║    APP CONTENT     ║▓▓▓█
  █▓▓║                    ║▓▓▓█
  █▓▓╚════════════════════╝▓▓▓█
  ░█▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓█░
  ░░██████████████████████████░░

  ░ = transparent
  █ = black outline
  ▓ = shading/color
```

---

## 6. Dialogs & Alerts

### Standard Alert

```
╔═══════════════════════════════════════════════════════╗
║                                                       ║
║   ⚠️      Alert message goes here. This explains     ║
║           what happened and what action is needed.    ║
║                                                       ║
║                          ┌────────┐   ╔════════════╗  ║
║                          │ Cancel │   ║     OK     ║  ║
║                          └────────┘   ╚════════════╝  ║
╚═══════════════════════════════════════════════════════╝
```

- Icon on left (32x32): Stop, Caution, Note
- Message text: Chicago 12px
- Buttons right-aligned
- Default button has thick border
- Modal with desktop dimming (optional)

### Alert Icons

```
  STOP (Error)         CAUTION (Warning)      NOTE (Info)
  ┌──────────┐         ┌──────────┐          ┌──────────┐
  │ ██████   │         │    ▲     │          │   ██     │
  │ █ ░░ █   │         │   ╱!╲    │          │  ████    │
  │ █ ░░ █   │         │  ╱   ╲   │          │   ██     │
  │ █ ░░ █   │         │ ╱─────╲  │          │          │
  │ ██████   │         │ ───────  │          │   ██     │
  └──────────┘         └──────────┘          └──────────┘
```

### Permission Prompt (Post-Urbit Specific)

```
╔═══════════════════════════════════════════════════════════════╗
║ ┌────┐                                                        ║
║ │ 🔑 │  "ExampleApp" wants to access your clipboard           ║
║ └────┘                                                        ║
║                                                               ║
║  This will allow the app to read text and images you copy.    ║
║                                                               ║
║  ┌─────────────────────────────────────────────────────────┐  ║
║  │ ○  Allow once                                           │  ║
║  │ ○  Allow for this session                               │  ║
║  │ ○  Always allow                                         │  ║
║  └─────────────────────────────────────────────────────────┘  ║
║                                                               ║
║                        ┌──────────┐   ╔═══════════════════╗   ║
║                        │  Deny    │   ║      Allow        ║   ║
║                        └──────────┘   ╚═══════════════════╝   ║
╚═══════════════════════════════════════════════════════════════╝
```

---

## 7. Shell Layout

### Main Shell Window

```
┌─────────────────────────────────────────────────────────────────────────┐
│  🍎  File  Edit  View  Apps  Window  Help                         │░░│ │
├─────────────────────────────────────────────────────────────────────────┤
│▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓│
│▓                                                                      ▓│
│▓   ┌────────┐   ┌────────┐   ┌────────┐   ┌────────┐                 ▓│
│▓   │        │   │        │   │        │   │        │                 ▓│
│▓   │  📁    │   │  📧    │   │  📝    │   │  🌐    │                 ▓│
│▓   │        │   │        │   │        │   │        │                 ▓│
│▓   └────────┘   └────────┘   └────────┘   └────────┘                 ▓│
│▓    Files        Mail        Notes       Browser                      ▓│
│▓                                                                      ▓│
│▓   ┌────────┐   ┌────────┐   ┌────────┐   ┌────────┐                 ▓│
│▓   │        │   │        │   │        │   │        │                 ▓│
│▓   │  ⚙️    │   │  🗑️    │   │  📊    │   │  ➕    │                 ▓│
│▓   │        │   │        │   │        │   │        │                 ▓│
│▓   └────────┘   └────────┘   └────────┘   └────────┘                 ▓│
│▓   Settings      Trash      Activity    Install App                   ▓│
│▓                                                                      ▓│
│▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓│
├─────────────────────────────────────────────────────────────────────────┤
│  4 apps installed │ Memory: 245MB / 512MB │ 🟢 Connected              │
└─────────────────────────────────────────────────────────────────────────┘
```

### Menu Bar

```
┌─────────────────────────────────────────────────────────────────────────┐
│  🍎  File  Edit  View  Apps  Window  Help                    │▶ 3:45 PM│
└─────────────────────────────────────────────────────────────────────────┘
```

- Height: 20px
- Background: white or light gray
- Active menu: inverted (black bg, white text)
- Apple menu: Post-Urbit logo
- Right side: status icons, clock

### Menu Dropdown

```
          ┌─────────────────────────┐
          │ New Window        ⌘N   │
          │ Open...           ⌘O   │
          ├─────────────────────────┤
          │ Close             ⌘W   │
          ├─────────────────────────┤
          │▓▓▓▓Get Info▓▓▓▓▓▓▓⌘I▓▓▓│ ← Selected
          │ Sharing...             │
          ├─────────────────────────┤
          │ Quit              ⌘Q   │
          └─────────────────────────┘
                    ░░░░░░░░░░░░░░░░░ ← Shadow
```

- 1px black border
- Drop shadow (1px offset, gray)
- Keyboard shortcuts right-aligned
- Separator: dotted or solid line
- Selection: inverted colors

### App Tiles

```
┌──────────────────┐
│  ┌────────────┐  │
│  │            │  │
│  │   32x32    │  │
│  │   ICON     │  │
│  │            │  │
│  └────────────┘  │
│    App Name      │
│                  │
│  ● Running       │ ← Optional status
└──────────────────┘
```

- Size: ~80x100px
- Icon: centered, 32x32
- Label: below icon, Geneva 9px
- Selection: inverted or highlight border
- Running indicator: small dot or highlight

### Status Bar

```
┌─────────────────────────────────────────────────────────────────────────┐
│  4 items │ 128MB available │ 🟢 Network │ 🔒 Secure            │░░│▒▒│
└─────────────────────────────────────────────────────────────────────────┘
```

- Height: 20px
- Inset bevel
- Left: item count, memory
- Right: status icons, resize handle

---

## 8. App Window (in webview)

Apps run in separate webviews but should match the shell aesthetic:

```
┌─────────────────────────────────────────────────────────────┐
│▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓│
│┌─┐═══════════════ Example App ════════════════════════┌─┐│
│└─┘                                                    └─┘│
├─────────────────────────────────────────────────────────────┤
│                                                             │
│                                                             │
│                   [ App Content Area ]                      │
│                                                             │
│              Apps can use full color here                   │
│              but chrome should match System 7               │
│                                                             │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Recommendation**: Provide a CSS theme/library for app developers to maintain consistency.

---

## 9. Animations & Transitions

### Supported Animations (subtle, period-appropriate)

| Animation | Usage | Style |
|-----------|-------|-------|
| Window open | App launch | Zoom from icon (optional) |
| Window close | App close | Zoom to icon / collapse |
| Button press | Click feedback | Instant invert (no transition) |
| Menu open | Dropdown | Instant appear + shadow |
| Alert appear | Dialogs | Instant or subtle zoom |
| Progress | Loading | Barber-pole stripes |

### Animation Guidelines

- **No smooth transitions** for controls (period-inaccurate)
- **Instant state changes** for buttons, checkboxes, radio
- **Optional**: Window zoom animation (can be disabled)
- **Progress bars**: Animated stripe pattern is authentic

---

## 10. Responsive Considerations

While System 7 wasn't responsive, Post-Urbit needs to work on various screen sizes:

| Breakpoint | Behavior |
|------------|----------|
| < 640px | Single column, larger touch targets |
| 640-1024px | 2-3 column app grid |
| > 1024px | Full desktop layout |

**Scaling approach**:
- Keep pixel fonts crisp (no fractional scaling)
- Scale by integer multiples (1x, 2x) for retina
- Maintain 16px as base grid unit

---

## 11. Dark Mode (Optional Extension)

System 7 didn't have dark mode, but if desired:

```
┌─────────────────────────────────────────────────────┐
│  Name              │  Light    │  Dark              │
├─────────────────────────────────────────────────────┤
│  Background        │  #EEEEEE  │  #333333           │
│  Window bg         │  #FFFFFF  │  #444444           │
│  Text              │  #000000  │  #EEEEEE           │
│  Borders           │  #000000  │  #666666           │
│  Highlight         │  #000080  │  #6699CC           │
└─────────────────────────────────────────────────────┘
```

**Recommendation**: Implement as optional "After Dark" mode toggle.

---

## 12. Implementation Notes

### CSS Custom Properties

```css
:root {
  /* Colors */
  --color-white: #FFFFFF;
  --color-light-gray: #EEEEEE;
  --color-desktop: #DDDDDD;
  --color-medium-gray: #AAAAAA;
  --color-dark-gray: #888888;
  --color-charcoal: #555555;
  --color-black: #000000;
  --color-highlight: #000080;

  /* Typography */
  --font-chicago: "ChicagoFLF", system-ui, sans-serif;
  --font-geneva: "Geneva", sans-serif;
  --font-size-base: 12px;

  /* Spacing */
  --space-unit: 4px;
  --border-width: 1px;
  --bevel-light: var(--color-white);
  --bevel-dark: var(--color-dark-gray);

  /* Shadows */
  --shadow-window: 1px 1px 0 var(--color-charcoal);
  --shadow-menu: 1px 1px 0 var(--color-dark-gray);
}
```

### Recommended Libraries

- **ChicagoFLF**: Free Chicago font recreation
- **98.css / 7.css**: Reference implementations (adapt for Mac)
- **Custom component library**: Build Post-Urbit specific components

### Assets Needed

1. **Fonts**: ChicagoFLF, Geneva (or fallbacks)
2. **Icon set**: 32x32 and 16x16 pixel art icons
3. **Patterns**: Desktop patterns, scrollbar track, title bar stripes
4. **Cursors**: Arrow, hand, I-beam, watch (optional)

---

## 13. Component Checklist

### Shell Components

- [ ] Menu bar with dropdowns
- [ ] Window chrome (title bar, borders, controls)
- [ ] Desktop background with pattern
- [ ] App icon grid
- [ ] Status bar
- [ ] Alert dialogs (stop, caution, note)
- [ ] Permission prompt dialog
- [ ] Progress indicator
- [ ] Notification area

### Form Controls

- [ ] Button (standard, default, pressed, disabled)
- [ ] Checkbox
- [ ] Radio button
- [ ] Text input
- [ ] Text area
- [ ] Dropdown / popup menu
- [ ] Scrollbar
- [ ] Slider
- [ ] Progress bar
- [ ] Tabs

### Icons

- [ ] System icons (folder, document, trash, etc.)
- [ ] App state icons (running, installing, error)
- [ ] Permission icons (lock, key, shield)
- [ ] Action icons (add, remove, check, close)
- [ ] Alert icons (stop, caution, note)

---

## References

- [Macintosh Human Interface Guidelines (1992)](https://archive.org/details/apple-human-interface-guidelines-1992)
- [Susan Kare's original icon designs](https://kare.com/)
- [System 7 screenshots (GUIdebook)](https://guidebookgallery.org/screenshots/macos70)
- [Chicago font specimen](https://en.wikipedia.org/wiki/Chicago_(typeface))
- [Folklore.org - Mac development stories](https://folklore.org/)
