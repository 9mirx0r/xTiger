---
name: xTiger
description: A calm, springy night workbench for validating and updating Crusader Kings III mods.
colors:
  harbor-navy: "#0A1530"
  deep-chrome: "#071027"
  harbor-line: "#1B3366"
  harbor-line-strong: "#2E4C8F"
  harbor-surface: "#10275B"
  harbor-surface-line: "#24407F"
  row-glow: "#163272"
  text-bright: "#E8EEFB"
  text-soft: "#C8D8F5"
  text-muted: "#9DB0D6"
  text-faint: "#7F93BC"
  signal-blue: "#458AFD"
  signal-blue-hover: "#5B99FF"
  signal-blue-text: "#8FB6FF"
  chart-paper: "#F6F8FC"
  ink-blue: "#1F5FD6"
  good-mint: "#6FD3A6"
  sev-fatal: "#FF5C7A"
  sev-error: "#FF8A5C"
  sev-warning: "#F2C94C"
  sev-untidy: "#B8C6E3"
  sev-tips: "#8FB6FF"
typography:
  display:
    fontFamily: "Geist, Segoe UI Variable Display, Segoe UI, system-ui, sans-serif"
    fontSize: "30px"
    fontWeight: 600
    lineHeight: 1.15
    letterSpacing: "-0.3px"
  headline:
    fontFamily: "Geist, Segoe UI Variable Display, Segoe UI, system-ui, sans-serif"
    fontSize: "22px"
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: "-0.2px"
  body:
    fontFamily: "Geist, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "Pixelify Sans, Geist, Segoe UI, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 500
    letterSpacing: "1.2px"
  code:
    fontFamily: "Geist Mono, Cascadia Mono, Consolas, ui-monospace, monospace"
    fontSize: "12px"
    fontWeight: 400
rounded:
  xs: "4px"
  sm: "6px"
  md: "8px"
  lg: "10px"
  xl: "12px"
  pill: "16px"
spacing:
  xs: "6px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "28px"
components:
  button:
    backgroundColor: "transparent"
    textColor: "{colors.text-bright}"
    rounded: "{rounded.lg}"
    padding: "0 16px"
    height: "40px"
  button-primary:
    backgroundColor: "{colors.signal-blue}"
    textColor: "{colors.deep-chrome}"
    rounded: "{rounded.lg}"
    padding: "0 16px"
    height: "40px"
  button-primary-hover:
    backgroundColor: "{colors.signal-blue-hover}"
  field:
    backgroundColor: "{colors.deep-chrome}"
    textColor: "{colors.text-bright}"
    rounded: "{rounded.lg}"
    padding: "0 14px"
    height: "40px"
  card:
    backgroundColor: "{colors.harbor-surface}"
    textColor: "{colors.text-bright}"
    rounded: "{rounded.xl}"
  chip:
    backgroundColor: "transparent"
    textColor: "{colors.text-soft}"
    rounded: "{rounded.pill}"
    height: "32px"
---

# Design System: xTiger

## Overview

**Creative North Star: "The Night Workbench"**

xTiger is the bench you work at late, next to the game and the editor: a deep navy surface lit by a single blue accent, with every tool within reach and nothing shouting. It is calm at rest and lively under the hand: controls press down, spring back with a small overshoot, and the results draw themselves in. A pixel font for the wordmark and eyebrow labels, and Qubis the blob as the maker's mark, keep the tool friendly without making it a toy.

The system is built for long sessions on dense information (reports, files, counts), so density and legibility come first and expression lives in details: the ASCII water behind the Home screen, severity colors as signal lights, the small brand-logo animations on assistant tiles. The light theme ("Chart Paper") is a first-class twin, not an inversion.

**Key Characteristics:**
- Deep-navy tonal layering instead of heavy shadows; one blue accent.
- Soft-edged, springy controls (8–12px radius, press-down scale, spring easing).
- Severity is the only multi-color vocabulary and always pairs color with a shape or label.
- Motion explains change (growing bars, popping badges, a bar that fills to Done) and is switched off by `prefers-reduced-motion`.

## Colors

A harbor at night: navy ground, slightly lighter navy surfaces, one signal-blue accent, and warm-to-cool severity lights.

### Primary
- **Signal Blue** (#458AFD dark, #1F5FD6 light "Ink Blue"): the only accent. Primary buttons, focus rings, the current-page marker in the rail, links. Hover lifts to #5B99FF (#2C6DE6 light). Text-sized accent uses #8FB6FF in dark.

### Neutral
- **Harbor Navy** (#0A1530; light #F6F8FC "Chart Paper"): app background.
- **Deep Chrome** (#071027; light #FFFFFF): title bar, rail, inputs and code wells; darker than the ground in dark theme.
- **Harbor Surface** (#10275B; light #FFFFFF) with **Surface Line** (#24407F; light #DCE3F0): cards, modals, tiles.
- **Harbor Line** (#1B3366) and **Strong Line** (#2E4C8F): dividers and control borders.
- **Row Glow** (#163272; light #E3ECFF): selected rows, hover wash, progress track.
- **Text** ramp: Bright #E8EEFB, Soft #C8D8F5, Muted #9DB0D6, Faint #7F93BC (light: #0B1A3D, #2C3E66, #56688F, #7A89A8).

### Severity (functional)
- **Fatal** #FF5C7A, **Error** #FF8A5C, **Warning** #F2C94C, **Untidy** #B8C6E3, **Tips** #8FB6FF (light: #B4123A, #C2410C, #8A5A00, #4B5B7E, #1F5FD6). Errors and fatals use a square dot; the others round.
- **Good Mint** #6FD3A6 (light #1E8A5A): fixed items, connected assistants, finished validation.

### Named Rules
**The One Blue Rule.** Signal Blue is the only accent; any other hue on screen means severity or a third-party brand logo.

**The Two Themes Rule.** Every color is a token on `:root` with a `[data-theme="light"]` twin. No raw hex in components except third-party brand marks.

## Typography

**Display Font:** Geist (with Segoe UI Variable Display)
**Body Font:** Geist (with Segoe UI Variable Text)
**Label/Pixel Font:** Pixelify Sans, only for the wordmark and eyebrows
**Mono:** Geist Mono (with Cascadia Mono, Consolas) for paths, keys, counts and code

**Character:** A clean, slightly technical grotesque for reading, with a pixel face as the one playful voice.

### Hierarchy
- **Display** (600, 30px, 26px in modal titles, -0.3px tracking): screen titles.
- **Headline** (600, 22px, -0.2px): the mod name on the results head.
- **Body** (400, 14px, ~1.5): everything else; lead paragraphs 16px, secondary notes 13px.
- **Label** (Pixelify Sans 500, 14px, +1.2px, uppercase): eyebrows and the wordmark. Overview captions are 11px, 700, +0.08em uppercase Geist.
- **Mono** (12px): file paths, report keys, counts.

### Named Rules
**The Pixel Is a Wink Rule.** Pixelify Sans appears only on the wordmark and eyebrows, never in body text or controls.

## Layout

A fixed frame: 40px title bar, 72px icon rail (48px buttons), and a flexible screen. Screens use 28px horizontal padding; the results screen stacks head, overview band, filter bar and a list/detail split. Settings is a 168px nav plus a column up to 980px; modals max out at 640px. Spacing runs on a loose 4/6/8/12/16/28px rhythm. Below 900px the "Where to look first" column hides; below 760px layouts collapse to one column. The app is built for a desktop window from about 800px up.

## Elevation & Depth

Tonal layering first: ground, chrome (darker), surface (lighter), row glow (selected). Shadows are reserved for things that lift or float: cards on hover, modals, toasts (`0 10px 30px rgba(0,0,0,0.35)`; a lighter blue-tinted one in the light theme). Primary buttons get an accent glow on hover (`0 6px 20px` of accent at 40%). The modal backdrop is `rgba(3,8,20,0.62)` with a 3px blur.

### Named Rules
**The Flat-Until-Touched Rule.** Surfaces are flat at rest; shadow or glow appears only in response to hover, focus or an overlay.

## Shapes

Soft corners, never sharp, never fully round except dots and pills: 4px severity segments, 6px tags and badges, 8px small buttons, 10px buttons and fields, 12px cards, tiles and logo plates, 16px pills. Borders are 1px in Harbor Line tones; selected items swap to Signal Blue with a faint ring. Severity dots are squares for error and fatal and circles otherwise.

## Components

### Buttons
- **Shape:** 10px radius, 40px high (large 48, small 32, extra small 28).
- **Primary:** Signal Blue fill, Deep Chrome text, 600 weight. **Secondary** is transparent with a Strong Line border that turns blue on hover.
- **Hover / Active:** a background wash or glow; press scales to 0.96 and springs back.

### Chips
- **Style:** 32px pill, tinted by severity when pressed (16% fill, 55% border), hollow dot when off; counts in mono.
- **State:** `aria-pressed` toggles; they stay in sync with the overview's severity segments.

### Cards / Containers
- **Corner Style:** 12px. **Background:** Harbor Surface. **Border:** 1px Surface Line.
- **Mod cards** lift 3px on hover with a shadow and a sheen across the thumbnail; the selected card takes a blue border.

### Inputs / Fields
- **Style:** 40px, 10px radius, Deep Chrome fill, Strong Line border.
- **Focus:** border to Signal Blue plus a 3px accent ring at 25%.

### Navigation
- 72px rail with 48px buttons (10px radius). The current page shows a Row Glow plate and a 3px blue marker that grows in from the left edge with a spring; a red dot badge pops in for pending requests.

### Results overview (signature)
- A stacked severity bar (14px high, segments flex by count, 3px gaps) that grows in from the left and filters when clicked; fixed/new delta pills; a "Where to look first" list of up to three files with proportional bars. The validating bar turns green and reads "Done" before the results appear.

### Assistant tiles (signature)
- A 46px logo plate (12px radius) per AI assistant with its real inline logo and a tiny behavior on hover or focus: Claude Code blinks and walks, Codex's cursor blinks, Cursor and VS Code turn and catch a sheen. Missing assistants show as greyscale chips.

## Do's and Don'ts

### Do:
- **Do** use tokens for every color and provide the light-theme twin.
- **Do** use `--ease-out` (cubic-bezier(0.22, 1, 0.36, 1)) for entrances and `--ease-spring` (cubic-bezier(0.34, 1.56, 0.64, 1)) for press and pop.
- **Do** keep animation CSS-only and let the global reduced-motion rule neutralize it.
- **Do** pair severity color with a label or a dot shape.
- **Do** keep all copy in plain English, short and literal.

### Don't:
- **Don't** add a second accent hue; severity and third-party logos are the only other colors.
- **Don't** use heavy resting shadows, gradient text, or glassmorphism beyond the modal backdrop blur.
- **Don't** load fonts, images or scripts from the network; CSP allows self only (`img-src 'self' data:`) and no inline scripts.
- **Don't** use Pixelify Sans for body text or controls.
- **Don't** let motion block work: the validation finish waits about 0.65s at most.
