# Architecture

## Pipeline

1. Extract
   - Read HWP/HWPX into a document model.
   - Detect question boundaries.
   - Split fused physical paragraphs into atoms.
   - Do not restyle or mutate content.

2. Classify
   - Assign semantic roles only.
   - Examples: `stem`, `stem_continuation`, `data_box`, `bogi_box`,
     `inline_table`, `choices`, `korean_set`.
   - Classification does not decide style.

3. Transform
   - Apply policies based on role and ownership.
   - Outside boxes: template owns style.
   - Box shells: template owns geometry, borders, label, and margins.
   - Box content: source owns paragraphs, runs, inline objects, tables,
     char/para style, tab structure, and lineSeg.

4. Compose
   - Merge source catalogs into the unified template.
   - Remap IDs once.
   - Splice transformed atoms into the unified body.

5. Render
   - Render the composed HWPX.
   - HWPX is canonical. Renderer differences are fixed in the renderer, not by
     changing source content.

## Non-Negotiable Rule

Box content is not normalized into template role styles.

The only allowed transformation for source content inside a box is ID remapping
so references resolve in the composed document. Any color, font, paragraph
margin, tab, lineSeg, picture, equation, or nested table mutation must be an
explicit renderer or source-normalization bug fix, never part of shell wrapping.
