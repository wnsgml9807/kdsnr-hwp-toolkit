//! Post-composition fixes applied to a composed question document.
//!
//! Ports the per-unit post-processing the legacy Python splitter ran after
//! injecting source paragraphs into a template (the steps that fix real
//! document correctness, not rhwp-specific render workarounds). Each function
//! is single-responsibility and operates on the composed `Document` in place.

use crate::model::control::Control;
use crate::model::document::Document;

/// Strip the template's default `tabStopVal`/`tabStopUnit` (legacy
/// `_strip_templet_tabstopval`).
///
/// The bundled templates' `<hp:secPr>` carry `tabStopVal=4000`; source files
/// don't set it and rely on the implicit default derived from `tabStop`. When
/// source paragraphs are injected into the template's section, that 4000 leaks
/// onto them and Hancom lays out their tabs (e.g. choice markers `① …`) at the
/// wrong interval. Flag the section so the serializer omits the attributes.
pub fn strip_default_tab_stop(doc: &mut Document) {
    for section in &mut doc.sections {
        section.section_def.omit_default_tab_stop = true;
        for para in &mut section.paragraphs {
            for ctrl in &mut para.controls {
                if let Control::SectionDef(sd) = ctrl {
                    sd.omit_default_tab_stop = true;
                }
            }
        }
    }
}

/// Empty the template's page footers (legacy `_strip_template_footer_scope_defs`).
///
/// The science/social templates' `<hp:footer>` carries a decorative sample
/// table; preserved as-is it overlays a stray box on every per-question
/// preview. Per-question output doesn't need footers/page numbers, so clear the
/// footer body while leaving the header (subject title + rule) intact. The
/// control itself is kept so the carrier paragraph's inline-control ordering is
/// undisturbed.
pub fn clear_footers(doc: &mut Document) {
    for section in &mut doc.sections {
        for para in &mut section.paragraphs {
            for ctrl in &mut para.controls {
                if let Control::Footer(footer) = ctrl {
                    footer.paragraphs.clear();
                }
            }
        }
    }
}
