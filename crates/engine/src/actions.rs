//! Multi-step custom actions, orchestration, and tool execution for PdfCraft.
//! Coordinates between OCR text extraction (`pdfcraft-ocr`), row boundary estimation,
//! and annotation injection (`pdfcraft-annot`) while recording changes in the session undo stack.

use crate::{DocId, Edit, Session};
use pdfcraft_annot::highlight::{HighlightAnnotation, HighlightBuilder, HighlightStyle, RgbaColor};
use pdfcraft_ocr::zero_detection::{self, OcrTextBlock, ZeroRowRect};
use std::fmt;

/// Errors that can occur during multi-step engine action execution.
#[derive(Debug)]
pub enum ActionError {
    /// Document ID was not found in the active session.
    DocumentNotFound(DocId),
    /// Operation requested an invalid page index.
    PageOutOfRange { page: usize, total: usize },
    /// Active document's permission security handler denies modifications.
    PermissionDenied(&'static str),
    /// Native OCR failed to process the page bitmap or extract text tokens.
    OcrEngineFailure(String),
    /// Annotation generation or appearance stream construction failed.
    AnnotationFailure(String),
    /// Action identifier does not match any registered runner.
    UnknownAction(String),
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DocumentNotFound(id) => write!(f, "Document session {id:?} not found"),
            Self::PageOutOfRange { page, total } => {
                write!(f, "Page index {page} is out of bounds (total pages: {total})")
            }
            Self::PermissionDenied(reason) => write!(f, "Permission denied: {reason}"),
            Self::OcrEngineFailure(err) => write!(f, "OCR engine failed: {err}"),
            Self::AnnotationFailure(err) => write!(f, "Annotation generation failed: {err}"),
            Self::UnknownAction(name) => write!(f, "Unknown action identifier: {name}"),
        }
    }
}

impl std::error::Error for ActionError {}

/// Return value from an action run, describing the mutation outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum ActionResult {
    /// Zero-score rows detected and highlighted.
    HighlightedZeroRows {
        page: usize,
        detected_count: usize,
        highlight_boxes: Vec<ZeroRowRect>,
    },
    /// OCR pass completed across one or more pages.
    OcrCompleted {
        pages_processed: usize,
        words_recognized: usize,
    },
    /// General mutation with a human-readable summary.
    Success(String),
}

/// Orchestrates user-facing and automation commands that span multiple subsystems.
pub struct ActionRunner;

impl ActionRunner {
    /// Dispatches a command by string ID onto a target document and page.
    pub fn execute(
        action_id: &str,
        session: &mut Session,
        doc_id: DocId,
        page_idx: usize,
    ) -> Result<ActionResult, ActionError> {
        match action_id {
            "ocr.highlight_zero_rows" | "tool.highlight_zero_rows" => {
                Self::highlight_zero_score_rows(session, doc_id, page_idx)
            }
            "ocr.recognize" | "ocr.recognize_page" => {
                Self::recognize_page_text(session, doc_id, page_idx)
            }
            "annot.clear_page" => Self::clear_page_annotations(session, doc_id, page_idx),
            other => Err(ActionError::UnknownAction(other.to_string())),
        }
    }

    /// Primary implementation for scanning the page with PdfCraft OCR, calculating
    /// student rows with zero scores, and generating amber highlight annotations across them.
    pub fn highlight_zero_score_rows(
        session: &mut Session,
        doc_id: DocId,
        page_idx: usize,
    ) -> Result<ActionResult, ActionError> {
        // 1. Verify document presence and permission flags
        let (page_width, page_height) = {
            let doc = session
                .get(doc_id)
                .ok_or(ActionError::DocumentNotFound(doc_id))?;

            if !doc.allows_modification() {
                return Err(ActionError::PermissionDenied(
                    "Document security prohibits adding annotations or modifying content",
                ));
            }

            let page_count = doc.page_count();
            if page_idx >= page_count {
                return Err(ActionError::PageOutOfRange {
                    page: page_idx,
                    total: page_count,
                });
            }

            // Extract page dimensions in PDF points (defaulting to Standard US Letter if not specified)
            doc.page_dimensions(page_idx).unwrap_or((612.0, 792.0))
        };

        // 2. Retrieve existing OCR blocks or extract text lines directly from the page
        let detected_blocks: Vec<OcrTextBlock> = {
            let doc = session
                .get(doc_id)
                .ok_or(ActionError::DocumentNotFound(doc_id))?;

            // Pull layout tokens extracted during document rendering or OCR pass
            let tokens = doc.extract_page_text_tokens(page_idx);
            if tokens.is_empty() {
                // If the page does not yet have an OCR/text layer, execute native OCR
                let rendered_bitmap = doc
                    .render_page_bitmap(page_idx, 150.0)
                    .map_err(|e| ActionError::OcrEngineFailure(e.to_string()))?;

                pdfcraft_ocr::engine::recognize_bitmap(&rendered_bitmap)
                    .map_err(|e| ActionError::OcrEngineFailure(e.to_string()))?
            } else {
                tokens
                    .into_iter()
                    .map(|tok| OcrTextBlock {
                        text: tok.text,
                        x_min: tok.bbox[0],
                        y_min: tok.bbox[1],
                        x_max: tok.bbox[2],
                        y_max: tok.bbox[3],
                        confidence: tok.confidence.unwrap_or(1.0),
                    })
                    .collect()
            }
        };

        // 3. Run the zero-score detection algorithm to calculate full-width row bands
        let highlight_bands = zero_detection::detect_zero_rows(
            &detected_blocks,
            page_width as f32,
            page_height as f32,
        );

        let detected_count = highlight_bands.len();
        if detected_count == 0 {
            return Ok(ActionResult::HighlightedZeroRows {
                page: page_idx,
                detected_count: 0,
                highlight_boxes: Vec::new(),
            });
        }

        // 4. Construct highlight annotations with an orange/amber grading appearance
        let style = HighlightStyle {
            color: RgbaColor::new(1.0, 0.55, 0.0, 0.35),
            author: "PdfCraft Grading Tool".to_string(),
            subject: "Zero Score Row".to_string(),
        };

        let mut annotations: Vec<HighlightAnnotation> = Vec::with_capacity(detected_count);
        for band in &highlight_bands {
            let annot = HighlightBuilder::new()
                .rect(band.x, band.y, band.width, band.height)
                .style(style.clone())
                .contents(format!(
                    "Flagged zero score row: Y=[{:.1}..{:.1}]",
                    band.y,
                    band.y + band.height
                ))
                .build()
                .map_err(|e| ActionError::AnnotationFailure(e.to_string()))?;

            annotations.push(annot);
        }

        // 5. Commit as an atomic edit into the document session (supports undo/redo)
        let edit = Edit::AddAnnotations {
            page: page_idx,
            annotations: annotations
                .into_iter()
                .map(|a| a.into_raw_annotation())
                .collect(),
        };

        session
            .apply(doc_id, edit)
            .map_err(|e| ActionError::AnnotationFailure(e.to_string()))?;

        Ok(ActionResult::HighlightedZeroRows {
            page: page_idx,
            detected_count,
            highlight_boxes: highlight_bands,
        })
    }

    /// Runs page OCR and builds invisible search text overlay.
    pub fn recognize_page_text(
        session: &mut Session,
        doc_id: DocId,
        page_idx: usize,
    ) -> Result<ActionResult, ActionError> {
        let doc = session
            .get(doc_id)
            .ok_or(ActionError::DocumentNotFound(doc_id))?;

        let rendered_bitmap = doc
            .render_page_bitmap(page_idx, 200.0)
            .map_err(|e| ActionError::OcrEngineFailure(e.to_string()))?;

        let blocks = pdfcraft_ocr::engine::recognize_bitmap(&rendered_bitmap)
            .map_err(|e| ActionError::OcrEngineFailure(e.to_string()))?;

        let words_recognized = blocks.iter().map(|b| b.text.split_whitespace().count()).sum();

        session
            .apply(
                doc_id,
                Edit::AttachOcrTextLayer {
                    page: page_idx,
                    tokens: blocks
                        .into_iter()
                        .map(|b| crate::OcrToken {
                            text: b.text,
                            bbox: [b.x_min, b.y_min, b.x_max, b.y_max],
                            confidence: Some(b.confidence),
                        })
                        .collect(),
                },
            )
            .map_err(|e| ActionError::OcrEngineFailure(e.to_string()))?;

        Ok(ActionResult::OcrCompleted {
            pages_processed: 1,
            words_recognized,
        })
    }

    /// Clears annotations from the target page.
    pub fn clear_page_annotations(
        session: &mut Session,
        doc_id: DocId,
        page_idx: usize,
    ) -> Result<ActionResult, ActionError> {
        session
            .apply(doc_id, Edit::ClearAnnotations { page: page_idx })
            .map_err(|e| ActionError::AnnotationFailure(e.to_string()))?;

        Ok(ActionResult::Success(format!(
            "Cleared annotations on page {}",
            page_idx + 1
        )))
    }
}