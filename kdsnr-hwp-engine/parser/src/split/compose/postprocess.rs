//! Post-composition fixes applied to a composed question document.

use crate::model::control::Control;
use crate::model::document::Document;

/// Drop the template's default `tabStopVal`/`tabStopUnit`. The bundled templates'
/// `<hp:secPr>` carry `tabStopVal=4000`; source paragraphs rely on the implicit
/// default, so the leaked 4000 lays their tabs (e.g. choice markers `① …`) at the
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

/// Empty the template's page footers. The science/social templates' `<hp:footer>`
/// carries a decorative sample table that would overlay a stray box on every
/// unit. Clear the footer body but keep the control, so the carrier's
/// inline-control ordering is undisturbed.
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
