//! Stable identifiers and source references.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SectionId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParagraphId {
    pub section: SectionId,
    pub index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ControlId {
    pub paragraph: ParagraphId,
    pub index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StyleId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceRef {
    Section(SectionId),
    Page(PageId),
    Paragraph(ParagraphId),
    Control(ControlId),
    Style(StyleId),
}
