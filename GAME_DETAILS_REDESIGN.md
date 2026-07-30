# NOVARA — Game Details Redesign Specification
Version: 1.0
Status: Approved Design Direction

---

# Purpose

This document defines the visual redesign of the Game Details page.

This is NOT a feature specification.

It is NOT a backend specification.

It is NOT a component rewrite.

It is purely a visual redesign intended to make NOVARA feel like a premium commercial game launcher.

This document is the source of truth for the Game Details page.

If the current implementation differs from this specification, this specification wins.

---

# Philosophy

The current Game Details page works.

It is functional.

However it still feels like a dashboard.

The redesign should completely change that perception.

The user should immediately feel like they are viewing a premium game page rather than an information page.

The interface should disappear.

The artwork should become the primary focus.

Think:

• Steam Library
• Ubisoft Connect
• PlayStation
• Xbox PC App
• Battle.net

Not:

• Admin dashboard
• CRUD application
• Settings screen
• Analytics page

The emotional response should be:

"This feels like a modern commercial game launcher."

---

# Non Goals

Do NOT

- redesign backend
- change IPC
- change routing
- change state management
- change commands
- change database
- change business logic
- change data models

This is strictly a frontend redesign.

---

# Overall Composition

Forget the current layout.

Do not evolve it.

Rebuild it.

The first screen should be dominated by one continuous hero.

Everything important belongs inside that hero.

Avoid independent floating sections.

Avoid stacked dashboard cards.

Think in terms of composition rather than containers.

The hero should occupy nearly the entire first viewport.

The artwork should become the page.

---

# Visual Hierarchy

The importance of elements should be:

1 Hero artwork

2 Cover

3 Title

4 Platform & Genre

5 Description

6 Metadata

7 Primary Actions

8 Tabs

9 Remaining content

Every level should naturally lead into the next.

Nothing should compete with the title.

Nothing should compete with the artwork.

---

# Hero

The hero is the identity of the page.

Requirements:

• Large cinematic artwork.
• Full-width.
• Tall.
• Immersive.
• Crisp.
• Minimal blur.

Use layered gradients only for readability.

Never allow gradients to completely hide the artwork.

The user should still appreciate the game's artwork.

The hero should not end abruptly.

Fade naturally into the page.

---

# Cover Artwork

The cover is one of the primary visual anchors.

Requirements:

Large.

Elegant.

High quality.

Inside the hero.

Not underneath the hero.

The cover should visually belong to the artwork.

It should feel physically attached.

Strong but soft shadow.

Rounded corners.

No thick borders.

---

# Logo / Title

The game logo should be preferred when available.

Otherwise use the text title.

The title must be significantly larger than every other piece of typography.

Never compress it.

Give it breathing room.

Avoid wrapping when possible.

---

# Platform Badges

Platform

Genre

Year

Completion

Status

These should become lightweight pills.

Compact.

Secondary.

They should support the title.

Not compete with it.

---

# Description

The description belongs directly beneath the badges.

Limit it to approximately three lines.

Gracefully truncate longer descriptions.

Muted typography.

Readable line height.

This provides emotional context before actions.

---

# Metadata

Developer

Publisher

Release Date

Last Played

Playtime

These should become elegant metadata rows.

Do NOT present them as oversized statistic cards.

Spacing should separate them.

Not borders.

Avoid unnecessary icons.

---

# Actions

Play is the primary action.

Every other action is secondary.

Actions:

Play

Favorite

Achievements

Saves

Mods

Refresh Metadata

Remove

Requirements:

Consistent height.

Consistent spacing.

One row.

Play should immediately attract attention.

Everything else should feel calm.

---

# Statistics

Do not create dashboard widgets.

If statistics exist they should integrate naturally into the hero or Overview.

Avoid isolated cards whose only purpose is displaying a single value.

---

# Tabs

Overview

Achievements

Artwork

Saves

Mods

Activity

Tabs should be visually lightweight.

Minimal.

Elegant.

Thin underline.

The active tab should feel refined rather than loud.

---

# Overview

The Overview page should remain.

Achievements.

About.

Notes.

Installations.

Existing functionality remains.

The redesign focuses on presentation.

Not features.

---

# Cards

Cards should feel premium.

Use subtle borders.

Large corner radius.

Soft elevation.

Minimal visual noise.

Whitespace should define sections.

Not heavy outlines.

---

# Spacing

Generous.

Comfortable.

Premium.

Never attempt to maximise information density.

Leave intentional empty space.

Whitespace is part of the design.

---

# Typography

Strong hierarchy.

Large title.

Readable description.

Muted metadata.

Comfortable line spacing.

High contrast.

Avoid oversized labels.

---

# Shadows

Soft.

Natural.

Realistic.

Never excessive.

The interface should feel calm.

---

# Motion

Minimal.

Elegant.

Short fades.

Subtle hover states.

Gentle transitions.

No flashy animations.

No unnecessary motion.

---

# Responsive Behaviour

The layout should gracefully scale.

The hero should remain dominant.

The cover should scale proportionally.

Text should wrap intelligently.

Buttons should wrap only when absolutely necessary.

Nothing should overlap.

---

# Accessibility

Keyboard navigation must continue to work.

Focus indicators remain.

Screen reader support must remain.

Contrast must remain WCAG compliant.

---

# Design Principles

Every decision should optimise for:

Visual hierarchy

Composition

Balance

Whitespace

Premium feel

Immersion

Readability

Consistency

The page should feel expensive.

---

# Final Quality Check

Before considering the redesign complete verify:

✓ Hero dominates the page.

✓ The game artwork is the first thing users notice.

✓ The title is the second thing users notice.

✓ The Play button is the third thing users notice.

✓ The interface disappears behind the game.

✓ No part of the page resembles an admin dashboard.

✓ The page feels like software produced by a AAA game company.

If any of these are not true, continue refining the layout until they are.

---

# Implementation Instructions

Before writing a single line of code:

1. Read this document completely.
2. Ignore the current Game Details layout.
3. Mentally reconstruct the page from this specification.
4. Explain your proposed layout in detail before implementation.
5. Only begin coding after your plan fully matches this document.
6. During implementation, favour composition and visual hierarchy over preserving existing CSS.
7. Do not stop after the first working version. Iterate until the page feels like a polished commercial launcher.
8. Treat this document as the design source of truth. Do not invent alternative layouts unless technically necessary.