# kdsnr-hwp-toolkit

Clean-room pipeline for KSAT HWP/HWPX question extraction, unified-template
composition, and PNG rendering.

The key contract is explicit:

- Outside box content: use the unified template role styles.
- Box shell: use the unified template shell geometry and labels.
- Inside box content: preserve source document content styles and layout.
- Rendering: HWPX is the source of truth; if PNG differs, fix the renderer.

This repository intentionally separates extraction, classification,
transformation, composition, and rendering. The old `flap-hwp-parser` mixed
those concerns, which made every input-specific failure look like a local
formatting bug.
