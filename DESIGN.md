# Nexus Design System

Reference contract: `/Volumes/work/ios2/iDescriptor`, especially
`src/ui/Theme.qml`, `src/ui/AppSidebar.qml`, `src/ui/SidebarDestinationButton.qml`,
and the shipped macOS screenshots in `resources/repo/`.

## 1. Atmosphere & Identity

Nexus is a quiet macOS utility surface: compact, legible, and layered with tonal
shifts rather than decorative effects. The signature is the iDescriptor-style
sidebar: a subdued platform surface with soft-blue selection, neutral text, and a
single system-blue accent instead of a saturated navigation block.

## 2. Color

Shared colors live in `app/qt/qml/Theme.qml`. New shared UI must consume these
roles rather than introduce another palette.

| Role | QML token | Light | Dark | Usage |
| --- | --- | --- | --- | --- |
| Window | `bg` | `#f5f5f7` | `#1f1f22` | Main application background |
| Surface | `surface` | `#ffffff` | `#2c2c2e` | Controls and grouped content |
| Elevated | `surfaceElevated` | white at 88% | white at 8% | Popovers / elevated panels |
| Text | `label` | `#1d1d1f` | `#f5f5f7` | Primary text |
| Muted text | `secondary` | `#6e6e73` | `#a1a1a6` | Secondary labels |
| Tertiary text | `tertiary` | `#8e8e93` | `#8e8e93` | Hints / disabled metadata |
| Accent | `blue` | `#0a84ff` | `#0a84ff` | Selection, links, primary controls |
| Hover | `sideHover` | black at 5.5% | white at 8% | Neutral hover state |
| Pressed | `pressed` | black at 8.5% | white at 12% | Pressed state |
| Selection | `selectionSoft` | blue at 16% | blue at 22% | Selected rows / navigation |
| Selection stroke | `selectionStroke` | blue at 28% | blue at 34% | Selected outlined rows |
| Separator | `separator` | black at 8% | white at 10% | Hairlines / region dividers |
| Control stroke | `controlStroke` | black at 10% | white at 13% | Inputs / cards |
| Success | `green` | `#34c759` | `#30d158` | Connected / successful |
| Warning | `orange` | `#ff9500` | `#ff9f0a` | Warning status |
| Error | `red` | `#ff3b30` | `#ff453a` | Destructive / failed |

Rules:
- System blue is the only general interaction accent.
- Selection is soft and tonal; reserve solid blue for primary controls.
- Separate surfaces with tonal contrast plus a one-pixel low-alpha stroke. Do not add generic drop shadows.

## 3. Typography

Primary stack: `SF Pro Text`, `SF Pro Display`, `PingFang SC`, then platform
fallbacks. Mono stack: `SF Mono`, `Menlo`, `Monaco`, `Consolas`.

| Level | Size | Weight | Usage |
| --- | ---: | --- | --- |
| Page title | 20 px | DemiBold | Tool / page heading |
| Section title | 15 px | DemiBold | Dialog and section heading |
| Body / control | 13 px | Normal–Medium | Navigation, controls, body text |
| Secondary | 12 px | Normal–Medium | Metadata, helper text |
| Caption | 11 px | Medium/DemiBold | Section labels, compact status |

Avoid all-caps except short section labels. Data identifiers and addresses may
use the mono stack.

## 4. Spacing & Layout

Base unit: **4 px**. Recurrent spacing intents are 4, 8, 12, 16, 20, 24, and 32
px. Optical 1–2 px adjustments are allowed for icon alignment.

- Expanded sidebar: 200 px; selected rows remain readable down to the existing 150 px resize floor.
- Sidebar row: 36 px with 8 px outer inset and 10 px inner horizontal padding.
- Main shell owns the application height. Sidebar and status regions remain fixed while their existing content regions own scrolling.
- Default grouped-card radius: 10 px; compact controls / sidebar selection: 8 px.
- Preserve the current minimum width behavior; labels must elide or wrap rather than create horizontal overflow.

## 5. Components

### App Shell
- **Structure:** fixed sidebar + main workspace + existing bottom status region.
- **Surface:** `bg`; sidebar uses `sidebarBackground` and `separator`.
- **States:** light/dark; expanded/collapsed sidebar.
- **Accessibility:** preserve platform window semantics and keyboard reachability.

### Sidebar Destination
- **Structure:** 18 px icon + 13 px label in a 36 px row.
- **Default:** transparent background, `label` text, `icon`/`secondary` icon tone.
- **Hover:** `sideHover`.
- **Pressed:** `pressed`.
- **Selected:** `selectionSoft` background, blue icon, normal primary text; the destination selection itself is fill-only.
- **Focus:** visible system-blue outline; the outline represents keyboard focus, not selection.
- **Motion:** 160 ms color transition.

### Grouped Card / Row
- **Surface:** `surface`, `surfaceElevated`, or `groupedBackground` according to hierarchy.
- **Radius:** 10 px outer card; 8 px compact inner controls.
- **Stroke:** one pixel `controlStroke`/`separator`.
- **Hover:** neutral `sideHover`; no elevation jump.

### Button / Dialog Action
- **Height:** 30–32 px for compact desktop controls.
- **Primary:** solid `blue` with white label.
- **Secondary:** `controlBg` plus subtle stroke; hover uses neutral fill.
- **Danger:** solid `red` only for destructive confirmation.
- **States:** default, hover, pressed, focus, disabled.

### Data Table / Status Dock
- **Density:** 12–13 px text, compact rows, one-pixel separators.
- **Selection:** tonal blue or semantic connected tint; avoid solid navigation-blue blocks.
- **Numbers / addresses:** mono stack where scanning benefits.

## 6. Motion & Interaction

| Type | Duration | Easing | Usage |
| --- | ---: | --- | --- |
| Micro | 160 ms | OutCubic | Hover, selection, sidebar width |
| Standard | 220 ms | InOutQuad / OutCubic | Popover / panel state changes |

Motion communicates state only. Do not add decorative looping animation. Existing
functional busy indicators may continue to animate. Keep keyboard and pointer
states equivalent.

## 7. Depth & Surface

Strategy: **tonal-shift + subtle stroke**. The reference does not depend on broad
drop shadows. The window, sidebar, grouped surfaces, rows, and popovers should
read as neighboring materials through low-alpha tonal differences and hairline
strokes.

## 8. Accessibility Constraints & Existing Debt

Target WCAG 2.2 AA where Qt rendering permits measurement. All interactive
elements retain accessible names, keyboard activation, and visible focus. Text
is not made smaller during alignment.

Existing debt: older QML files still contain one-off fallback colors and spacing
values predating this design contract. This alignment does not expand that debt;
new shared visual decisions belong in `Theme.qml`, and touched components should
prefer those tokens. A repository-wide token migration is intentionally outside
this UI-style alignment because it would broaden the behavioral diff without
improving the requested reference match.
