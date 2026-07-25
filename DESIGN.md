# NOVARA — Design System & Visual Language Specification

> **Single Source of Truth** for NOVARA's User Interface and User Experience.  
> Document Version: 1.0.0 | Official Design System Specification.

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

### 8.6 Platform Identity Tokens

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
| **Hero Title** | 40px | 800 (ExtraBold) | 1.06 | -1.0px | Sentence | Hero banner main game titles |
| **Game Details Title**| 30px | 800 (ExtraBold) | 1.15 | -0.6px | Sentence | Main title on game details page |
| **Page Title** | 24px | 800 (ExtraBold) | 1.20 | -0.5px | Sentence | Top-level view section headers |
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
 40px   [ 5.0x ]  Hero banner internal content padding
```

---

## 11. Border Radius Scale

Consistent corner rounding establishes visual relationships across component scales:

- `--radius-sm` (`8px`): Form inputs, standard buttons, keycaps, artwork preview slots.
- `--radius-md` (`12px`): Game cards, navigation items, list containers, stat cards, toast popups.
- `--radius-lg` (`16px`): Main content panels, game detail covers, modal popups.
- `--radius-xl` (`22px`): Dashboard hero banners, featured artwork frames.
- **Pill / Circular (`999px` / `50%`):** Platform badges, search inputs, segmented filter tabs, quick action overlay buttons.

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
- **Standard Scale:** 14px (inline action text), 16px (navigation, stat pills), 20px (action buttons), 24px–32px (empty state headers).
- **Color Inheritance:** Icons inherit `currentColor` by default, adopting `--accent-bright`, `--success`, `--warning`, or `--danger` during active or status states.

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
- **Icon Action Buttons:** Square or circular 36×36px bounds designed specifically for single-icon controls.

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
- Houses search icon left and keyboard shortcut hint `<kbd>` right (`Ctrl+K`).

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

The Game Details page offers a rich visual layout for individual titles:

- **Hero Banner Area:** Cinematic horizontal banner aspect ratio (`21 / 9`), overlaid with dark bottom gradient scrim for title readability.
- **Overlapping Cover Art:** Vertical cover (`2 / 3` ratio) overlaps the hero banner bottom edge smoothly to create visual depth.
- **Title Block & Primary CTAs:** Clear typographic hierarchy displaying large game title (30px ExtraBold), platform badge, completion status dropdown, and primary Play CTA button.

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

### 23.2 Skeleton Loading States
Content loading states present styled background cards with linear shimmer animations sweeping across headline and media blocks.

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

---

## 27. Component Usage Rules

1. **Token Strictness:** Components MUST consume visual tokens from the design system palette. Never introduce hardcoded hex strings or arbitrary CSS values.
2. **Media Primitives:** Game artwork, banners, and logos must render through standardized media components to enforce consistent aspect ratios and progressive loading blurs.
3. **Semantic States:** Always use semantic status tokens (`--success`, `--warning`, `--danger`) for state badges rather than generic decorative colors.

---

## 28. Anti-Patterns & Practices to Avoid

- **DO NOT mix conflicting visual identities:** Avoid loud, noisy neon aesthetics. Keep luminous violet/cyan accents focused on visual focal points.
- **DO NOT hardcode ad-hoc CSS colors or spacing values:** Rely strictly on established design tokens.
- **DO NOT break standard media aspect ratios:** Always preserve standard vertical cover (`2 / 3`) and hero banner (`21 / 9`) proportions.
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
