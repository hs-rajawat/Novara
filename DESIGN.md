# NOVARA — Design System & Visual Language Specification

> **Single Source of Truth** for NOVARA's User Interface and User Experience.  
> Document Version: 1.1.0 | Official Design System Specification.

---

## 1. Document Overview & Purpose

**NOVARA** is a premium, privacy-first desktop game launcher built to consolidate multi-platform PC game libraries (Steam, Epic, GOG, Xbox, Ubisoft, Emulators, Custom) into a unified, visual-first application.

This document serves as the official design system specification for NOVARA. It establishes the visual identity, design tokens, component standards, interaction patterns, accessibility guidelines, and spatial rules necessary to ensure aesthetic rigor and functional consistency across all interface experiences.

---

## 2. Design Philosophy

NOVARA's design language is built on four core pillars:

1. **Cinematic Immersion:** Games are high-fidelity visual art. The interface acts as a dark, atmospheric canvas that defers to high-resolution game artwork, hero banners, logos, and ambient color spill.
2. **Space-Age Precision:** Deep space slate surfaces, crisp typography, clean micro-borders, and subtle electric violet/cyan highlights create a sleek, futuristic high-tech aesthetic.
3. **Tactile Desktop Responsiveness:** Built specifically for desktop OS environments. Controls respond instantaneously to input with spring physics, elevation shifts, subtle glow effects, and precision cursor feedback.
4. **Privacy-First & Offline Resilience:** UI components display gracefully in fully offline environments without missing icon glyphs, broken images, or layout distortion.

---

## 3. Cohesive Visual Identity

NOVARA's visual identity unites **Cinematic Dark Mode** with **Space-Age Precision**. Rather than cluttering the interface with aggressive neon graphics, light and color are applied with strict intentionality:

- **Atmospheric Canvas:** Surfaces range from deep void black (`#070a12`) to rich slate layers (`#0c111d` – `#1b2338`), establishing strong contrast and spatial depth.
- **Luminous Violet & Cyan Accents:** The signature 135° linear gradient—from Electric Violet (`#7c5cff`) to Electric Cyan (`#38bdf8`)—is reserved for high-priority actions, active state highlights, and primary brand indicators.
- **Focused Specular Lighting:** Subtle glows and glass translucency serve as functional visual cues indicating interactive focus, active navigation, or elevated surface depth.
- **Brand Mark & Logotype:** The brand mark is a 32×32px rounded square with a vibrant 135° linear gradient fill (`#7c5cff` to `#38bdf8`), featuring a floating gamepad motif and an ambient glow aura. Logotype headings use display typography with a gradient text clipping effect.

---

## 4. Design Priorities

When making design decisions, balance trade-offs according to this explicit order of priority:

1. **Clarity:** Information, hierarchy, and affordances must be immediately understandable.
2. **Consistency:** Visual patterns, spatial rules, and interaction behaviors must remain predictable across every view.
3. **Accessibility:** The interface must support clear contrast, visible focus states, legibility, and full keyboard navigation.
4. **Performance:** Layouts and transitions must feel fast, smooth, and lightweight on desktop hardware.
5. **Delight:** Micro-interactions, spring animations, and specular glows elevate the experience without causing visual distraction.

---

## 5. UX Principles

1. **Artwork-First Hierarchy:** Visual media drives game discovery. High-contrast typography and platform badges complement artwork without obscuring key visual focal points.
2. **Predictable Layout Boundaries:** Navigation rails, headers, and viewports maintain explicit spatial bounds so fast scrolling or filtering never distorts layout structure.
3. **Non-Intrusive Background State:** Background sweeps (library scans, status integrity checks, save backups) report status via quiet notifications without modal blocking or interrupting user workflow.
4. **Immediate Spatial Continuity:** Navigation transitions use persistent visual anchors and smooth spring-based indicators to reinforce spatial orientation across views.
5. **Forgiving Interactions:** Non-destructive actions (favorites, completion status, view filters) apply instantly; destructive operations (removing games, purging save data) require explicit confirmation overlays.

---

## 6. Design Decision & Evolution Guidelines

### 6.1 Evaluation Principles
When designing new components or evaluating interface changes, apply these decision criteria:

1. **Respect the Artwork:** Interface chrome must never overpower or unnecessarily crop game media.
2. **Token-Driven Design:** Never introduce custom visual values (colors, margins, radiuses) when an established design token exists.
3. **Spatial Stability:** Prevent layout shifts during asynchronous data loading or content filtering.
4. **Semantic Color Application:** Restrict vibrant accent colors to active states, focus rings, status indicators, and primary calls to action. Neutral slate tones handle structure and secondary text.
5. **Desktop-Native Focus:** Prioritize precision pointer hover states, full keyboard navigation, clear visual focus rings, and high-DPI scaling tolerance.

### 6.2 Evolution Guidelines for Future UI
- **Reuse Existing Patterns:** Always exhaust established visual patterns before creating a new component variant.
- **Prefer Consistency over Novelty:** A familiar, coherent interface is more valuable than an inventive one-off design.
- **Minimize Visual Noise:** Eliminate unnecessary borders, badges, or backgrounds that do not serve a functional purpose.
- **Extend, Don't Diverge:** When new features require new UI patterns, extend the existing design language rather than introducing an alternate visual style.

---

## 7. Visual Language & Surface Elevation Model

NOVARA structures visual depth through a dark, multi-layered surface stack. Depth is communicated via surface luminosity, micro-borders, glass translucency, and subtle top specular reflections.

```
┌────────────────────────────────────────────────────────┐
│  Glass Backdrop Overlay (--bg-glass)                   │ Layer 4: Sticky TopBar, Notification Toasts, Modals
├────────────────────────────────────────────────────────┤
│  Elevated Surface (--bg-3: #1b2338)                    │ Layer 3: Active Segmented Tabs, Keycaps, Scrollbars
├────────────────────────────────────────────────────────┤
│  Card & Input Surface (--bg-2: #131a2b)                │ Layer 2: Game Cards, Form Fields, Stat Cards
├────────────────────────────────────────────────────────┤
│  Main Surface Panel (--bg-1: #0c111d)                  │ Layer 1: Sidebar Rail, View Panels, List Containers
├────────────────────────────────────────────────────────┤
│  Base Canvas Layer (--bg-0: #070a12)                   │ Layer 0: Root Desktop Window Surface
└────────────────────────────────────────────────────────┘
```

---

## 8. Color Palette & Semantic Tokens

### 8.1 Surface Tokens

| Token | Value | Visual Purpose |
|---|---|---|
| `--bg-0` | `#070a12` | Deep space black base layer; root window background. |
| `--bg-1` | `#0c111d` | Deep slate panel background; sidebar rail, base containers, list rows. |
| `--bg-2` | `#131a2b` | Midnight slate fill; game cards, input fields, interactive containers. |
| `--bg-3` | `#1b2338` | Elevated slate fill; active segmented controls, keycaps, scrollbar thumbs. |
| `--bg-hover` | `#212c46` | Interactive hover state for slate surfaces and ghost controls. |
| `--bg-glass` | `rgba(12, 16, 28, 0.72)` | Translucent glass backdrop for sticky headers, toasts, and overlays. |

### 8.2 Border & Specular Lighting Tokens

| Token | Value | Visual Purpose |
|---|---|---|
| `--border-soft` | `rgba(148, 163, 196, 0.09)` | Subtle structural panel borders and internal dividers. |
| `--border-strong` | `rgba(148, 163, 196, 0.20)` | High-contrast borders for input focus, active cards, and toasts. |
| `--inner-light` | `inset 0 1px 0 rgba(255, 255, 255, 0.04)` | Top-edge specular reflection simulating light on elevated edges. |

### 8.3 Typography Color Tokens

| Token | Value | Visual Purpose |
|---|---|---|
| `--text-primary` | `#eef1f8` | High-contrast ice white for headings, primary labels, and active items. |
| `--text-secondary` | `#99a3ba` | Muted slate for body text, subtitles, metadata, and inactive nav labels. |
| `--text-tertiary` | `#5c6479` | Deep charcoal for input placeholders, timestamps, and section headers. |
| `--text-inverse` | `#070a12` | High-contrast dark text placed over vibrant gradient buttons or badges. |

### 8.4 Accent & Brand Tokens

| Token | Value | Visual Purpose |
|---|---|---|
| `--accent` | `#7c5cff` | Electric Violet; primary focus indicators, active checks, key highlights. |
| `--accent-bright` | `#a18bff` | Vibrant Violet text and icon highlights; active section indicators. |
| `--accent-2` | `#38bdf8` | Electric Cyan; secondary accent, informational states, secondary badges. |
| `--accent-soft` | `rgba(124, 92, 255, 0.16)` | Violet background wash for active navigation items and count pills. |
| `--accent-2-soft` | `rgba(56, 189, 248, 0.14)` | Cyan background wash for automatic detection badges and info stats. |
| `--gradient-accent` | `linear-gradient(135deg, #7c5cff 0%, #38bdf8 100%)` | Primary brand gradient; CTA buttons, brand logotype, active switches. |
| `--accent-glow` | `0 4px 20px rgba(124, 92, 255, 0.35)` | Luminous aura shadow for primary actions and brand focal points. |

### 8.5 Status & Integrity Tokens

| Token | Value | Visual Purpose |
|---|---|---|
| `--success` | `#4ade80` | Emerald Green; installed game state, completed progress, success toast. |
| `--success-soft` | `rgba(74, 222, 128, 0.14)` | Soft green wash for success badges and stat card backgrounds. |
| `--warning` | `#facc15` | Solar Gold; favorite indicators, backlog completion state, warning toast. |
| `--warning-soft` | `rgba(250, 204, 21, 0.14)` | Soft gold wash for active favorite controls and backlog pills. |
| `--danger` | `#f87171` | Crimson Red; missing/deleted installation status, destructive buttons, error toast. |
| `--danger-soft` | `rgba(248, 113, 113, 0.12)` | Soft red wash for error badges and destructive action backgrounds. |

### 8.6 On-Artwork & Borderless Surface Tokens

Chrome that sits on game artwork is **solid, not glass**. A translucent pill washes
out over bright artwork and its fill shifts with whatever happens to be behind it;
a filled chip with a hairline and a little elevation reads deliberately at any
brightness. Borderless card interiors use faint washes so sections are defined by
space rather than rules.

Glass (`backdrop-filter`) remains correct for chrome over *scrolling content* — the
sticky top bar, toasts, modal backdrops — but not for chrome over artwork.

| Token | Value | Visual Purpose |
|---|---|---|
| `--on-art-solid` | `#151a26` | Filled surface for pills and buttons over artwork. |
| `--on-art-solid-hover` | `#1f2637` | Hover state for on-artwork controls. |
| `--on-art-line` | `rgba(255, 255, 255, 0.10)` | Hairline edge for on-artwork surfaces. |
| `--on-art-line-strong` | `rgba(255, 255, 255, 0.20)` | Hover/active edge for on-artwork surfaces. |
| `--on-art-shadow` | `0 2px 8px rgba(0, 0, 0, 0.40)` | Subtle elevation lifting chrome off the art. |
| `--pill-neutral-bg` / `-line` | `#151a26` / `rgba(255,255,255,.11)` | Launcher badge: neutral dark. |
| `--pill-accent-bg` / `-text` / `-line` | `#241d47` / `#bbabff` / `rgba(124,92,255,.42)` | Genre badges: brand accent. |
| `--pill-gold-bg` / `-text` / `-line` | `#2d2411` / `#f5d67a` / `rgba(250,204,21,.38)` | Progress badge, favourite state: amber. |
| `--on-art` | `#ffffff` | Titles and control labels over artwork. |
| `--on-art-strong` | `rgba(255, 255, 255, 0.92)` | Primary on-artwork text. |
| `--on-art-muted` | `rgba(255, 255, 255, 0.76)` | Description copy over artwork. |
| `--on-art-faint` | `rgba(255, 255, 255, 0.64)` | Metadata labels over artwork; the contrast floor for on-artwork text. |
| `--surface-1` | `rgba(255, 255, 255, 0.03)` | Borderless row/tile wash inside cards. |
| `--surface-2` | `rgba(255, 255, 255, 0.05)` | Hover wash and icon marks inside cards. |
| `--surface-line` | `rgba(255, 255, 255, 0.07)` | Hairline divider, progress track, quiet pill edge. |
| `--card-border` | `rgba(255, 255, 255, 0.05)` | Premium card hairline border. |
| `--card-border-hover` | `rgba(255, 255, 255, 0.09)` | Premium card hover border. |

### 8.7 Platform Identity Tokens

| Platform Source | Indicator Color | Tone Label |
|---|---|---|
| **Steam** | `#66c0f4` | Steam Cyan |
| **Epic Games** | `#e2e8f0` | Epic White |
| **GOG Galaxy** | `#b389f9` | GOG Purple |
| **Xbox PC** | `#6fd66f` | Xbox Green |
| **Ubisoft** | `#6fb8ff` | Ubisoft Sky Blue |
| **Battle.net** | `#7ea6ff` | Battle.net Blue |
| **Emulators** | `#4adecb` | Emulator Teal |
| **Manual / Custom**| `#5c6479` | Slate Neutral |

The tone drives either a 6px dot or, where the badge leads with a launcher mark
(§14.1, §21.6), the mark's own colour — never both, so adding branding never adds
weight.

---

## 9. Typography Scale

NOVARA uses a clean typography hierarchy engineered for high legibility, crisp structural weight, and tabular numeric alignment.

### 9.1 Font Families
- **Body & Sans UI (`--font-sans`):** `"Inter", "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif`
- **Headings & Display (`--font-display`):** `"Inter", "Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif`
- **Monospace & Numerics (`--font-mono`):** `"Cascadia Code", "JetBrains Mono", "Fira Code", Consolas, monospace`

### 9.2 Type Scale Reference

| Style Role | Font Size | Weight | Line Height | Letter Spacing | Case | Target Usage |
|---|---|---|---|---|---|---|
| **Game Details Hero** | `clamp(38px, 4.4vw, 68px)` | 800 (ExtraBold) | 1.02 | -0.03em | Sentence | Game Details hero title; logo preferred when available (§21.4) |
| **Hero Title** | 40px | 800 (ExtraBold) | 1.06 | -1.0px | Sentence | Dashboard hero banner game titles |
| **Page Title** | 24px | 800 (ExtraBold) | 1.20 | -0.5px | Sentence | Top-level view section headers |
| **Display Metric** | 34px | 800 (ExtraBold) | 1.00 | -0.03em | Numeric | Completion and achievement percentages on Game Details |
| **Section Heading**| 20px / 16px | 700 / 650 | 1.30 | -0.2px | Sentence | Modal headers, sub-section titles |
| **Subheader / Brand**| 15px / 15.5px | 600 / 700 | 1.35 | -0.2px | Sentence | Topbar title, sidebar brand mark |
| **Body Standard** | 14px | 400 (Regular) | 1.55 | 0.0px | Sentence | Paragraph copy, user notes, general text |
| **Interactive Label**| 13px / 14px | 500 (Medium) | 1.40 | 0.0px | Sentence | Buttons, nav labels, dropdown options |
| **Card Title** | 13px | 600 (SemiBold) | 1.35 | 0.0px | Sentence | Game card titles, stat headings |
| **Small Label / Meta**| 12px | 500 (Medium) | 1.45 | 0.0px | Sentence | Tooltips, secondary metadata lines |
| **Micro / Status** | 11px / 10.5px | 600 (SemiBold) | 1.20 | +0.4px | Mixed | Status badges, chips, micro labels |
| **Section Eyebrow** | 10.5px / 11.5px| 700 (Bold) | 1.20 | +1.2px / +1.4px| UPPERCASE | Nav category headers, hero eyebrows |
| **Keycaps (`kbd`)** | 10px | 500 (Medium) | 1.00 | 0.0px | UPPERCASE | Keyboard shortcut helpers |

---

## 10. Spacing System (8px Grid Scale)

All component dimensions, margins, and padding operate on an **8px grid** with a **4px sub-grid step**:

```
  4px   [ 0.5x ]  Fine adjustments (icon-to-text gaps, keycap padding, dot margins)
  8px   [ 1.0x ]  Base spacing (button vertical padding, input gaps, chip margins)
 12px   [ 1.5x ]  List row vertical padding, metadata row gaps, section margins
 16px   [ 2.0x ]  Panel internal padding, search bar padding, standard card gap
 20px   [ 2.5x ]  Game grid spacing (20px gap), artwork slot gaps
 24px   [ 3.0x ]  Main content margins, page section gaps, toast offsets
 32px   [ 4.0x ]  Main view horizontal padding, hero banner bottom margin
 40px   [ 5.0x ]  Hero banner internal content padding, tab strip bottom margin, Game Details hero bottom margin, description-to-metadata separation
 48px   [ 6.0x ]  Large compositional separation
 64px   [ 8.0x ]  Game Details hero top reserve
```

The scale is exposed as `--space-1` … `--space-8`. The main view's gutter is
also tokenised (`--content-pad-x`, `--content-pad-y`) so a full-bleed element
can cancel it exactly rather than guessing — this is how the Game Details hero
reaches both window edges (§21.2).

---

## 11. Border Radius Scale

Consistent corner rounding establishes visual relationships across component scales:

- `--radius-sm` (`8px`): Form inputs, standard buttons, keycaps.
- `--radius-md` (`12px`): Game cards, navigation items, list containers, stat cards, toast popups, Game Details hero action controls.
- `--radius-lg` (`16px`): Main content panels, modal popups, artwork preview frames.
- `--radius-2xl` (`20px`): Game Details panel cards.
- `--radius-xl` (`22px`): Dashboard hero banners, featured artwork frames.
- **Fluid (`clamp(12px, 1.1vw, 18px)`):** Game Details hero cover, which scales with the poster.
- **Pill / Circular (`999px` / `50%`):** Platform badges, search inputs, segmented filter tabs, quick action overlay buttons, hero badges, completion state pills.

---

## 12. Elevation, Lighting & Shadows

Elevation communicates interactive readiness and surface hierarchy without visual noise.

```css
--shadow-1:   0 1px 0 rgba(255, 255, 255, 0.03) inset, 0 8px 24px rgba(0, 0, 0, 0.35);
--shadow-pop: 0 12px 32px rgba(0, 0, 0, 0.45), 0 0 0 1px rgba(124, 92, 255, 0.14), 0 0 28px rgba(124, 92, 255, 0.1);
--accent-glow: 0 4px 20px rgba(124, 92, 255, 0.35);
```

1. **Layer 1 Elevation (Panels & Lists):** `--shadow-1` combined with top-edge specular highlight (`--inner-light`). Provides static depth to flat surfaces.
2. **Layer 2 Elevation (Hovered Cards & Modals):** `--shadow-pop`. Combines drop shadow with a soft violet accent micro-glow.
3. **Layer 3 Elevation (Primary Action Aura):** `--accent-glow`. Reserved for primary CTA buttons, active switch toggles, and logotype branding.
4. **Composed Surfaces (Game Details):** Elevation is quieter where artwork leads. Panel cards use the specular top edge with a soft, wide drop shadow; the hero cover uses a wide shadow plus a specular hairline instead of a border; and the hero Play button uses a contained violet elevation rather than `--accent-glow`, so it reads as set into the page rather than lit up.

---

## 13. Motion & Interaction Guidelines

Animations in NOVARA are swift, tactile, and non-distracting.

### 13.1 Timing & Easing Guidelines
- **Fast Interaction (`--dur-fast: 0.15s`):** Hover color transitions, focus states, button presses. Easing: standard ease-out.
- **Medium Transition (`--dur-med: 0.28s`):** Sliding navigation indicator, card hover elevation, view entrance. Easing: `--ease-out` (`cubic-bezier(0.22, 1, 0.36, 1)`).
- **Spring Physics (`--ease-spring`):** `cubic-bezier(0.34, 1.45, 0.64, 1)`. Applied to quick action overlays, toggle switches, and navigation indicators.

### 13.2 Motion Keyframes & Transitions
- **View Entrance:** Subtle 0.28s fade-up (`translateY(8px)` to `0`, opacity `0` to `1`).
- **Loading Shimmer:** Linear background sweep across skeleton loading blocks.
- **Checkmark Activation:** Spring scale transform when checking boxes.

### 13.3 Reduced Motion Standard
All components MUST respect the user's OS reduced-motion preference by disabling non-essential transitions when `prefers-reduced-motion: reduce` is active.

---

## 14. Iconography Guidelines

NOVARA uses an inline, dependency-free stroke icon system based on modern 24×24 geometry.

- **Stroke Characteristics:** 2px stroke width, open end caps, rounded joins, fill set to `none`.
- **Standard Scale:** 13px (source badge marks), 14px (inline action text), 16px (navigation, stat pills), 20px (action buttons), 24px–32px (empty state headers).
- **Color Inheritance:** Icons inherit `currentColor` by default, adopting `--accent-bright`, `--success`, `--warning`, or `--danger` during active or status states.

### 14.1 Launcher & Source Marks
Each store has a mark (`src-steam`, `src-epic`, `src-gog`, `src-xbox`,
`src-ubisoft`, `src-battle`, `src-manual`, plus `gamepad` for emulators) used by
the source badge (§21.6) and tinted with that platform's tone (§8.7).

- **Simplified, Not Reproduced:** These are NOVARA's own geometric marks, drawn
  to *identify* a store at a glance rather than to reproduce its trademark. They
  share the 24×24 grid, 2px stroke and optical weight of every other icon, so a
  badge row of mixed sources reads evenly.
- **Local Only:** Drawn inline like the rest of the set — no icon font, no vendor
  logo files, nothing to fetch and nothing to miss offline (§28).
- **Graceful Fallback:** An unrecognised source code falls back to the generic
  `src-manual` mark and the neutral slate tone, so a future store renders
  correctly before it has a mark of its own.

---

## 15. Buttons & Interactive Controls

### 15.1 Button Hierarchy

```
┌────────────────────────────────────────────────────────┐
│  Primary Button Variant                                │  Gradient fill, luminous aura, shimmer hover line
├────────────────────────────────────────────────────────┤
│  Secondary Button Variant                              │  Card surface fill, subtle border, hover highlight
├────────────────────────────────────────────────────────┤
│  Ghost Button Variant                                  │  Transparent fill, subtle hover surface background
├────────────────────────────────────────────────────────┤
│  Hero Ghost Variant                                    │  Translucent dark fill, backdrop glass blur
├────────────────────────────────────────────────────────┤
│  Danger Button Variant                                 │  Soft red wash fill, crimson border & text
└────────────────────────────────────────────────────────┘
```

### 15.2 Button Sizing Standards
- **Small Sizing:** Compact 28px height, 12px font size; used for inline controls and tight toolbar actions.
- **Standard Sizing:** Standard 36px height, 13px font size; used for general interface actions.
- **Large Sizing:** Prominent 44px height, 14px font size, medium corner radius; used for primary view CTAs like "Play Now".
- **Hero Action Sizing:** 44px height (`--gd-control-h`), 13.5px font size, `--radius-md`, glass fill over artwork. Every control in the Game Details hero action row — including icon-only ones — shares this single height (§21.8).
- **Icon Action Buttons:** Square or circular 36×36px bounds designed specifically for single-icon controls (44×44px in the Game Details hero, to match the row).

---

## 16. Form Inputs & Selection Controls

### 16.1 Form Input Fields
- **Background & Border:** Midnight card surface (`--bg-2`) with soft border (`--border-soft`).
- **Corner Radius:** Small radius token (`--radius-sm`).
- **Focus Experience:** Border transitions to Electric Violet (`--accent`), accompanied by a soft glow focus ring (`--accent-soft`), background shifts to deep slate (`--bg-1`).
- **Placeholder:** Deep charcoal text (`--text-tertiary`).

### 16.2 Search Bar Control
- Fully rounded pill shape (`999px`).
- Expands smoothly on focus with standard ease-out motion.
- Houses search icon left and keyboard shortcut hint `<kbd>` right (`/`).
  A single-key hint is used deliberately in place of a chorded shortcut:
  NOVARA has no competing text-entry surface at the app level, so `/`
  reaches search in one keystroke and matches the convention users know
  from Steam, Discord and the browser. Implementations must ignore `/`
  while focus is inside an input, textarea or contenteditable element.

### 16.3 Toggles & Checkboxes
- **Custom Checkbox:** Rounded container with small radius token. Active state fills with primary brand gradient and displays spring-animated checkmark.
- **Toggle Switch:** Pill track with circular thumb that glides horizontally with spring physics upon activation.

---

## 17. Cards & Surface Containers

### 17.1 Game Cards
- **Structure:** Vertical layout containing cover artwork (standard vertical 2:3 aspect ratio), quick action overlay, title metadata, and platform pill badge.
- **Hover Behavior:** Elevates slightly, highlights border with soft violet accent, triggers smooth quick-action controls fade-in.

### 17.2 Stat Cards
- Grid layout adapting to container width.
- Houses 42×42px icon box with distinct color washes (violet, cyan, green, gold), uppercase stat label, and bold numeric value (`font-feature-settings: "tnum"`).
- **Scope:** A Dashboard and Analytics pattern. Deliberately not used on Game Details, where figures integrate into the hero composition as metadata rows instead (§21.7, §21.9).

### 17.3 Premium Panel Cards
- Used by Game Details below the hero: `--radius-2xl`, single hairline border (`--card-border`), specular top edge plus soft drop shadow, and generous internal padding (`clamp(20px, 2vw, 28px)`).
- Card headings are quiet: 14px SemiBold in `--text-primary`, with no icon and no accent colour, so the card's content leads.
- The card's primary affordance is a 34px outlined secondary button (transparent fill, `--border-strong` hairline, `--radius-md`), not a text link — a card action should look pressable. Text-button styling is reserved for affordances *inside* rows.
- Interiors are borderless. Row groups use `--surface-1` washes and 8px-grid gaps rather than dividers.

---

## 18. Sidebar & Navigation Experience

NOVARA's sidebar rail provides continuous spatial orientation using a synchronized gliding indicator pill:

- **Rail Geometry:** Fixed width (`216px`), pinned vertical layout.
- **Synchronized Item Height:** Every navigation item and indicator pill shares a single height metric.
- **Gliding Indicator:** Smooth sliding pill moving along item layout positions using spring physics.
- **Active Nav State:** Active item text adopts primary text color while its icon lights up with vibrant violet accent.

---

## 19. Header & TopBar Infrastructure

The sticky top bar anchors the main view header:

- **Dimensions & Position:** Fixed height (`58px`), sticky top position.
- **Glass Translucency:** Translucent glass backdrop (`--bg-glass`) with background blur effect.
- **Content Layout:** Current view title on left, responsive search input on right, status indicators.

---

## 20. Game Library Layout & Grid System

The library view organizes game collections cleanly:

- **Responsive Grid:** Fluid grid layout (`minmax(200px, 1fr)`) with 20px spacing gap.
- **Control Strip Toolbar:** Top flexbar housing segmented filter tabs with count badges, and sort dropdowns.
- **Horizontal Media Carousel:** Horizontal snap-scroll container with circular floating scroll navigation controls.

---

## 21. Game Details Page Experience

Game Details is NOVARA's flagship page and its most artwork-led surface. It is
composed, not stacked: a single continuous hero owns the first viewport and
carries the entire identity of the game, and the page only becomes conventional
UI below the fold. `GAME_DETAILS_REDESIGN.md` is the implementation
specification for this page; this section records the resulting design so the
two documents agree.

### 21.1 Composition Order
Visual weight descends in a fixed order, and each level leads into the next:
hero artwork → cover → title/logo → platform & genre → description →
metadata → primary actions → tabs → remaining content. Nothing competes with
the artwork, and nothing competes with the title.

### 21.2 Hero
- **Full-Bleed Canvas:** The hero cancels the main view's gutter to meet the
  top bar and both window edges. It carries no border and no corner radius.
- **Artwork Fits the Width, Never the Box:** Hero art is anchored to the top and
  scaled to the hero's *width*, taking its height from the source's own ratio.
  It is therefore **never cropped horizontally** — the full width of the key art
  is always visible, at any hero height. `object-fit: cover` was the previous
  behaviour and is explicitly rejected here: filling a box taller than the source
  meant scaling up and discarding a third of the image from the sides, which cut
  the composition the artist framed. Only a source *taller* than the hero is
  clipped, and then from the bottom, under the fade.
- **The Artwork Dissolves at Its Own Edge:** A gradient on the artwork element
  itself fades it to exactly `--bg-0` by its bottom edge, wherever the source's
  ratio puts that edge. The join with the page is therefore seamless by
  construction rather than by a percentage tuned for one aspect ratio.
- **Height Is Free of the Artwork:** Because the crop is width-driven, hero
  height no longer trades against artwork. It only decides how much of the
  composition sits on artwork versus on the fade beneath it. Set to `76vh` with a
  `560px` floor and a `52vw` ceiling — cinematic, but with the tab strip still
  inside the fold.
- **Composition Straddles the Two:** At 1080p the artwork occupies the top ~67%
  of the hero and the composition begins at ~40% down, so the logo, badges and
  the first lines of the description sit over real artwork and the metadata and
  actions descend into the fade. That overlap is what makes the hero read as one
  composition rather than a banner with a caption below it.
- **Readability Scrim, Not a Cover:** Since the artwork carries its own fade, the
  scrim is only what the text needs: a narrow wash behind the identity column
  clearing by two thirds of the width, a soft lift near the bottom, and a whisper
  of top shade for the back control. No blur anywhere (§6.1.1).
- **Nothing Overlaps the Boundary:** Every element of the composition lives
  inside the hero. There is no element straddling its lower edge.

### 21.3 Cover Artwork
Vertical cover (`2 / 3`) placed *inside* the hero, bottom-aligned with the text
column rather than with the action row — which lifts it higher in the frame and
lets the action row span beneath it (§21.4). Width is fluid (`--gd-cover-w`,
`clamp(176px, 15vw, 264px)`) and it remains a primary anchor — never reduced to
make room for text. Elevation comes from a wide soft shadow plus a top-edge
specular hairline — no border — so the poster reads as physically attached to the
artwork. The gap to the identity column (`clamp(28px, 3vw, 44px)`) is deliberately
generous: the logo must not read as glued to the poster's edge.

### 21.4 Left-Weighted Composition
Cover, logo, badges, description, metadata and actions form **one block**, not
six sections, and the page must feel compositionally balanced rather than
mathematically centred. The block has two stacked parts: a row holding the poster
and the text, and the action row spanning beneath both. Three rules hold it
together:
- A tight left gutter (`--gd-hero-pad-x`, `clamp(20px, 2.6vw, 44px)`) weights the
  block to the left edge instead of floating it in negative space.
- A bounded width (`min(1180px, 86%)`) stops the block short of the hero's right
  edge, leaving an open field of artwork there, released below `1500px` where a
  narrow hero has no open field to protect.
- The poster and the text column are bottom-aligned to each other, and the action
  row is their **sibling** rather than a child of the text column. That is what
  puts the row under the poster instead of indented past it, closes the dead space
  that used to sit beneath the cover, and lifts the poster higher in the frame.

The eye travels cover → logo → badges → description → metadata → Play in a
single unbroken descent.

### 21.5 Logo & Title
The game logo is **preferred** whenever one is available; the text title is the
fallback. When the logo renders, the `<h1>` remains in the document and is
hidden visually only, preserving the document outline and screen-reader
announcement. A logo that fails to decode falls back to the text title. The
title is the largest type in the application (§9.2) and is never compressed.

### 21.6 Badges
The badge row is **source → genre → genre → completion %**, and nothing else.

- **Source Badge First:** The first badge names where the game lives and leads
  with that store's mark (§14.1), tinted with its platform tone (§8.7). It is
  the fastest answer to "which launcher owns this?" and earns first position.
- **Every Badge Earns Its Place:** Release year and completion state are
  deliberately *not* badges — the release date is an About row (§21.13) and the
  completion state is not on this page at all (§21.14). A badge must answer a
  question no neighbouring element already answers.
- **Compact Values:** Completion reads `42%`, not `42% complete`. The unit is
  self-evident; the extra word only adds width.
- **Exception for Warnings:** Library-integrity states (§23.4) may repeat
  information found in the Installations panel, because a broken install is a
  warning rather than a restatement, and it appears only when something is wrong.

Badges are **solid** chips (§8.6), 32px tall with 14px of side padding — sized so a
launcher mark has room to be recognisable, which is what turns the source badge
from a tag into an identity badge. Colour carries the hierarchy:

| Badge | Treatment |
|---|---|
| Launcher | Neutral dark (`--pill-neutral-*`), with the store's 15px mark in its own platform tone (§8.7) |
| Genre | Brand accent (`--pill-accent-*`) |
| Completion % | Amber (`--pill-gold-*`), the same language earned achievement tiles speak |
| Integrity warning | Opaque danger or slate, per §23.4 |

They support the title; they never compete with it.

### 21.7 Description
Directly beneath the badges: muted, comfortable line height, clamped to three
lines (two on narrow widths) and truncated gracefully. Width is a *measure*
(`min(68ch, 100%)`), not the container — the paragraph ends on a comfortable line
length rather than running to the block's edge. A step and a half of separation
follows it, so the metadata reads as its own layer of information rather than a
fourth line of prose; whitespace does that work, not a rule.

### 21.8 Metadata
Developer, publisher, release date, last played and playtime render as label-
over-value metadata rows separated by whitespace alone — no borders, no icons,
no single-value cards. Rows with no data are omitted rather than shown as
"Unknown"; a game that has never been launched reads `Never` rather than
disappearing.

### 21.9 Actions
One row, one height (`--gd-control-h`, `44px`), spanning the **full composition
width** from the poster's left edge — not indented into the text column, which
left dead space beneath the cover.

**Three groups, one ratio.** The bar reads as `[Play] · [Favorite Achievements
Saves Mods] · [Refresh Remove]`, and that grouping is structural (`.gd-action-group`)
rather than hand-spaced: **24px between groups, 8px within them**, both off the
8px scale. The 3:1 ratio is what makes the grouping legible; no gap in the row is
set by hand.

**One component family.** Every control in the row — including Play and the two
icon-only utilities — carries the same base class, so height, corner radius, border
weight, type size, icon size (16px), icon-to-label gap and vertical alignment come
from a single rule. The utility pair is square (44×44), identically bordered, and
sits at the group's internal 8px spacing so Refresh and Remove read as one
connected control.

**Play earns attention without bulk.** It is the same height, radius, type size and
icon size as its neighbours; only the gradient, its own group and the whitespace
around it distinguish it. It carries no `min-width` and no larger type — deliberately,
so that at 50% zoom the eye lands on one saturated element while the remaining six
read as a single cohesive secondary group.

**Padding is uniform.** All labelled buttons share `--gd-btn-pad`, so they have equal
visual density despite different label lengths; Play takes 6px more per side for
presence. The token steps 18 → 15 → 12 → 10px as width tightens, so the row sheds
padding before it is ever allowed to wrap.

Refresh Metadata and Remove are icon-only with accessible names. Remove governs
library membership — it never deletes game files — and its copy says so.

### 21.10 Statistics
This page has no dashboard widgets. Figures integrate into the composition
instead: playtime and last-played as hero metadata, completion as a hero badge
with the state read-only here (§21.14), achievement progress as the
Achievements card's headline metric. Stat cards (§17.2) are a Dashboard and
Analytics pattern and are deliberately absent here.

### 21.11 Tabs
Overview, Achievements, Artwork, Saves, Mods, Activity. Visually lightweight:
no container, no count pills, a hairline `1.5px` accent underline that scales in
on the active tab. Roving focus with `ArrowLeft` / `ArrowRight` / `Home` / `End`,
`role="tablist"` semantics, and panels that cross-fade. The strip carries the
section nav and nothing else.

### 21.12 Overview & Cards
Four cards in a two-column grid, balanced by weight rather than by category:

```
Achievements  |  About
Notes         |  Installations
```

Cards use `--radius-2xl`, a single hairline border (`--card-border`), and soft
elevation; whitespace defines sections, not heavy outlines. Row groups inside
cards (installations, sessions, save profiles) are separated by surface wash and
space rather than dividers. Each card's primary affordance is an **outlined
button** (`.gd-btn-outline`), never a text link — a card action should look like
something you press.

**One purpose per card, strictly.** A card must not mix what the game reports with
what the user assigns. The Achievements card contains achievement data and nothing
else; the completion state it once shared space with is library management and
lives on the tab strip (§21.14). No two cards may communicate the same information
either.

**Achievements card structure**, in this exact order and containing nothing else:
1. Header: `Achievements`, with a `View All` outlined button at the
   upper right (disabled while there is nothing to view). The card title already
   says Achievements, so the button does not repeat it.
2. Headline metric: the percentage, followed by the word `Complete`.
3. Progress bar, on a real surface track (`--bg-3`) so the remaining distance
   reads as distance rather than as nothing.
4. Count line beneath the bar: `12 / 51 unlocked`.
5. Strip of achievement tiles: capped at eight, with a `+N` tile for the
   remainder.

**Tiles are collectibles, not absences.** Locked is the normal state of this strip
— most games are mostly unearned — so a locked tile must read as something waiting
to be claimed: a full surface step above the card (`--bg-2`), a real 1px border,
`--radius-md` corners, a legible `--text-secondary` lock at 18px, even spacing, and
a soft hover that lifts and brightens. Earned tiles are the one place on this page
gold is spent. A washed-out placeholder variant is explicitly rejected — it made
the shelf look broken rather than unearned.

**The card looks finished before the data exists.** With no achievements recorded
it shows `0%`, an empty bar, `0 / 0 unlocked` and a full shelf of locked tiles —
which is what an unplayed game looks like anyway. There is **no empty-state
message** in the Overview card, and **never an invented total**: `0 / 0` is true,
whereas `0 / 51` would be indistinguishable from real data to someone reading
their own library.

**Transition from the hero:** deliberately tight. The hero's bottom margin
(`--space-3`) and the tab strip's (`--space-4`) are the smallest gaps on the page,
so the tab strip reads as a divider between two parts of one page and the first
cards feel connected to the artwork above rather than floating below it.

### 21.13 About
Six short rows, readable in two or three seconds. Not a database dump — the target
is *scannable*, so values are marks and short labels rather than prose.

| Row | Form |
|---|---|
| Release date | Text. Deliberately **not** a hero badge; this is where it belongs. |
| Platform | OS marks — Windows, macOS, Linux, plus Steam Deck when reported. Text only if no mark exists for the value. |
| Genre | Up to three tags. Long genre lists are noise. |
| Features | Curated chips with marks: Single Player, Co-op, Multiplayer, Achievements, Cloud Saves — capped at four. |
| Languages | Three tags plus a `+N more` disclosure. |
| Controller | A gamepad mark plus `Full` / `Partial Controller Support`, normalised from the provider's own casing. |

- **Curated, not mirrored.** Steam ships twenty-odd categories per game, most of
  them plumbing (`Remote Play on Phone`, `Family Sharing`, `Stats`). Features shows
  only what a player cares about: how many people can play, and whether progress is
  tracked. The match table lives with the component so the rule is auditable.
- **Not shown:** the metadata provider's name (an implementation detail) and the
  engine (never populated by NOVARA's providers, so a permanently empty row).
- **No duplication:** developer and publisher live in the hero metadata (§21.8).
- **Full data retained.** `parseGameFacts` still extracts everything; only the
  display is edited, so nothing is lost when a row is trimmed or capped.
- **Empty state:** if no facts are available at all, a single sentence explains how
  to get them rather than leaving a blank card.

### 21.14 Library Status
The five completion states (Playing, Backlog, Completed, Abandoned, Unplayed) are
**absent from Game Details**. This page is about the game; where a
library-management control belongs globally is a separate decision, so it is not
relocated into a card, a tab strip or a status bar here.

Consequence, recorded deliberately: `set_completion` currently has no caller in the
UI, so the state is read-only for the user. It still changes on its own — the
backend derives `playing` when a session ends — and it is still read by the Library
filter strip, game cards, the Dashboard and the Dashboard hero. The command, its
IPC wrapper and the data are untouched and ready for whatever surface is chosen.

---

## 22. Modals, Overlays & Dialogs

- **Native Dialog Interactions:** Seamless integration with desktop OS native file dialogs for executables, covers, hero banners, and logos.
- **Web Overlay Modals:** Centered surface panel (`--bg-1`, strong border, large radius, elevated drop shadow) over dark translucent glass backdrop.
- **Modal Header & Footer:** Distinct header title block, scrollable body area, and action footer displaying Primary and Ghost buttons aligned right.
- **Focus & Keyboard Trap:** Modals capture focus and close when pressing `Escape` or selecting the backdrop.

---

## 23. Empty, Loading, and Error States

### 23.1 Empty View States
Centered layout featuring a glowing 64px or 76px icon container, headline text (16–20px), descriptive muted subtext (13px), and clear action CTA buttons (e.g., "Scan Launchers", "Add Game Manually").

**Scope:** This is a *view*-level pattern. Inside a card — notably the Game
Details panels — an empty state is a single muted sentence explaining what will
appear and how to get it, because a full illustration block reads as a hole in
the layout.

### 23.2 Skeleton Loading States
Content loading states present styled background cards with linear shimmer animations sweeping across headline and media blocks. Where a page is dominated by one composition, the skeleton instead occupies that composition's exact footprint (the Game Details hero) so arrival causes no layout shift.

### 23.3 Notification Toasts
- Positioned floating at bottom-right viewport margin (`bottom: 24px`, `right: 24px`).
- Translucent glass containers (`320px` width) displaying status icons and animated shrink progress timer bars.
- **Semantic Toast Types:** Success (`--success`), Info (`--accent-2`), Warning (`--warning`), Error (`--danger`).

### 23.4 Installation Health Badges
- **`installed`:** Emerald green status badge (`--success`).
- **`offline`:** Slate neutral status badge (storage drive disconnected; non-alarming).
- **`missing` / `deleted`:** Crimson red danger badge (`--danger`).

---

## 24. Accessibility Guidelines (a11y)

1. **Visible Focus Indicators:** Interactive elements must display a distinct 2px focus ring (`--accent`) with 2px offset during keyboard navigation (`:focus-visible`).
2. **Text Contrast Ratios:** Body text maintains a minimum contrast ratio of 4.5:1 against slate backgrounds. Headings meet 7:1.
3. **Tabular Numerals:** Playtime figures, percentages, dates, and session counts use `font-feature-settings: "tnum"` to prevent layout jitter during live counter updates.
4. **Full Keyboard Navigation:** All controls, card menus, filter pills, and modal dialogs are fully navigable via standard keyboard controls (`Tab`, `Enter`, `Space`, `Esc`).

---

## 25. Desktop OS & High-DPI UX Guidelines

1. **Windows OS Integration:** Native Segoe UI Variable font stack fallbacks, native titlebar region alignment, Cascadia Code monospace font rendering.
2. **Layout Boundaries:** Viewports, scroll regions, and panels define explicit boundary constraints to prevent double scrollbars.
3. **High-DPI Scaling Resilience:** Layout offsets avoid fractional pixel rounding artifacts at 125%, 150%, and 175% OS display scaling.
4. **Offline Asset Fallbacks:** Missing game media gracefully degrades to stylized typographic placeholder containers.

---

## 26. Responsive & Window Adaptation Behaviors

- **Sidebar Rail Scaling:** Maintains fixed width (`216px`) on standard displays, adapting to a compact rail (`64px`) on narrow window widths below 900px.
- **Game Grid Adaptation:** Fluid grid column recalculation maintaining card aspect ratios automatically.
- **Hero Frame Resizing:** Fluid hero frame height adapting proportionally with window width bounds.
- **Game Details Hero Adaptation:** The hero stays dominant at every size. Height, cover width and hero padding are driven by tokens overridden at breakpoints (narrow ≤1000px, short ≤720px) rather than by per-rule media queries, so all dependent geometry moves together. Below 820px the cover and identity stack vertically and the description clamps to two lines. Buttons wrap only when the row genuinely cannot fit.

---

## 27. Component Usage Rules

1. **Token Strictness:** Components MUST consume visual tokens from the design system palette. Never introduce hardcoded hex strings or arbitrary CSS values.
2. **Media Primitives:** Game artwork, banners, and logos must render through standardized media components to enforce consistent aspect ratios and progressive loading blurs.
3. **Semantic States:** Always use semantic status tokens (`--success`, `--warning`, `--danger`) for state badges rather than generic decorative colors.

---

## 28. Anti-Patterns & Practices to Avoid

- **DO NOT mix conflicting visual identities:** Avoid loud, noisy neon aesthetics. Keep luminous violet/cyan accents focused on visual focal points.
- **DO NOT hardcode ad-hoc CSS colors or spacing values:** Rely strictly on established design tokens. Chrome placed over artwork uses the on-artwork token set (§8.6), not raw rgba values.
- **DO NOT distort standard media aspect ratios:** Vertical covers are always `2 / 3` and hero artwork keeps its source ratio; media must never be stretched to fit. Where a container is taller than the source, fit the artwork to the container's *width* and fade its lower edge rather than scaling up and cropping the sides — a horizontal crop discards the composition the artist framed (§21.2).
- **DO NOT turn single values into dashboard widgets on artwork-led pages:** Integrate figures into the composition as metadata or badges instead (§21.7, §21.9).
- **DO NOT let interface chrome outrank the artwork on Game Details:** Nothing may compete with the hero artwork, the title, or the Play action, in that order (§21.1).
- **DO NOT use external icon fonts:** Use inline SVG primitives to preserve offline resilience.
- **DO NOT use browser-default focus outlines or plain red/blue controls.**

---

## 29. Component Quality Checklist

Before submitting a new UI component to NOVARA, verify that it satisfies every item in this quality checklist:

- [ ] **Token Alignment:** All colors, borders, surface fills, font sizes, and radiuses use standard design system tokens.
- [ ] **Visual Hierarchy:** Primary text uses `--text-primary`, secondary metadata uses `--text-secondary`, and section labels use `--text-tertiary`.
- [ ] **Interactive States:** Hover, active, disabled, and focus states (`:focus-visible`) are explicitly defined and tested.
- [ ] **Spatial Grid Compliance:** All margins, paddings, and gaps align with the 8px grid scale (or 4px sub-grid step).
- [ ] **Typography & Numerics:** Live numeric counters, dates, and durations include `font-feature-settings: "tnum"`.
- [ ] **Motion & Reduced Motion:** Animations use standard ease/duration tokens and respect `prefers-reduced-motion: reduce`.
- [ ] **Offline & Image Resilience:** Missing image media degrades gracefully to typographic placeholder containers without breaking layout height.
- [ ] **High-DPI Scale Tested:** Component renders crisply without pixel blur or border misalignment at 125% and 150% display scaling.
