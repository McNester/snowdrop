# Snowdrop Pre-defense Presentation

This folder contains a short commercial-style pre-defense presentation for Snowdrop.

## Files

- `snowdrop_predefense_pitch.tex` - main Beamer presentation
- `speaker_notes_ru.md` - short Russian speaking notes for each slide

## Recommended build

Use `xelatex` because the slides contain Russian text and rely on `fontspec`.

```bash
xelatex -interaction=nonstopmode snowdrop_predefense_pitch.tex
xelatex -interaction=nonstopmode snowdrop_predefense_pitch.tex
```

## Presentation profile

- 10 slides
- black-and-white visual style
- dark textured wall / spotlight-inspired background
- minimal text, more diagrams and visual structure
- Beamer overlay animations for progressive reveal

## Main message

The presentation is intentionally framed as a product/business pitch rather than as a formal thesis defense:

- what Snowdrop is
- why it matters
- how the architecture works
- how migration works
- where it is applicable
- what performance signal already exists
