//! High-precision zero-score row detector for scanned grading sheets and exams.
//! Analyzes OCR bounding boxes, detects zero score patterns, computes median text
//! heights to estimate row boundaries, and merges contiguous horizontal bands.

use regex::Regex;
use std::sync::LazyLock;

/// Precompiled regex patterns matching score notations equal to zero.
static ZERO_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // e.g., "0 / 10", "0/5"
        Regex::new(r"(?i)\b0\s*/\s*\d+").unwrap(),
        // e.g., "points: 0", "pts = 0", "pts: 0 / 10"
        Regex::new(r"(?i)\b(?:points?|pts?)\s*[:=]?\s*0\b").unwrap(),
        Regex::new(r"(?i)\b(?:points?|pts?)\s*[:=]?\s*0\s*/").unwrap(),
        // e.g., "A: 0", "B = 0"
        Regex::new(r"(?i)\b[A-D]\s*[:=]?\s*0\b").unwrap(),
        // standalone zero score column format: "Score: 0"
        Regex::new(r"(?i)\bscore\s*[:=]?\s*0\b").unwrap(),
    ]
});

/// Pattern to identify if the detected zero score appears on the primary/header line of a rubric item.
static TOP_LINE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[A-D]\s*[:=]").unwrap()
});

/// Pattern to normalize OCR misidentifications of uppercase 'O' or lowercase 'o' in place of '0'.
static O_SLASH_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b[Oo]\s*/\s*(\d+)").unwrap()
});

static O_POINTS_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(points?|pts?)\s*[:=]?\s*[Oo]\b").unwrap()
});

/// A single OCR text block / token with spatial coordinates on the page.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrTextBlock {
    pub text: String,
    pub x_min: f32,
    pub y_min: f32,
    pub x_max: f32,
    pub y_max: f32,
    pub confidence: f32,
}

impl OcrTextBlock {
    pub fn new(text: impl Into<String>, x_min: f32, y_min: f32, x_max: f32, y_max: f32) -> Self {
        Self {
            text: text.into(),
            x_min,
            y_min,
            x_max,
            y_max,
            confidence: 1.0,
        }
    }

    #[inline]
    pub fn height(&self) -> f32 {
        (self.y_max - self.y_min).abs()
    }

    #[inline]
    pub fn y_center(&self) -> f32 {
        (self.y_min + self.y_max) * 0.5
    }
}

/// A computed highlight rectangle covering the full page width for a detected student row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZeroRowRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Internal representation of a candidate score mark.
struct CandidateZeroBox {
    y_center: f32,
    box_height: f32,
    is_top_line: bool,
}

/// Normalizes common OCR misreadings of zero as letter 'O' in numeric and score contexts.
pub fn normalize_ocr_score_text(input: &str) -> String {
    let step1 = O_SLASH_PATTERN.replace_all(input, "0/$1");
    let step2 = O_POINTS_PATTERN.replace_all(&step1, "$1: 0");
    step2.into_owned()
}

/// Scans OCR text blocks and computes merged full-width row bounding boxes.
pub fn detect_zero_rows(
    blocks: &[OcrTextBlock],
    page_width: f32,
    page_height: f32,
) -> Vec<ZeroRowRect> {
    if blocks.is_empty() {
        return Vec::new();
    }

    let mut candidate_boxes = Vec::new();
    let mut detected_heights = Vec::with_capacity(blocks.len());

    for block in blocks {
        let clean_text = block.text.trim();
        if clean_text.is_empty() || block.confidence < 0.2 {
            continue;
        }

        let h = block.height().max(1.0);
        detected_heights.push(h);

        let normalized = normalize_ocr_score_text(clean_text);

        let mut matches_zero = false;
        for pattern in ZERO_PATTERNS.iter() {
            if pattern.is_match(&normalized) {
                matches_zero = true;
                break;
            }
        }

        if matches_zero {
            let is_top = TOP_LINE_PATTERN.is_match(&normalized);
            candidate_boxes.push(CandidateZeroBox {
                y_center: block.y_center(),
                box_height: h,
                is_top_line: is_top,
            });
        }
    }

    if candidate_boxes.is_empty() {
        return Vec::new();
    }

    // Determine estimated row height from the median detected text height
    let default_row_h: f32 = 48.0;
    let estimated_row_h = if !detected_heights.is_empty() {
        detected_heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = detected_heights.len() / 2;
        let median_h = if detected_heights.len() % 2 == 0 {
            (detected_heights[mid - 1] + detected_heights[mid]) * 0.5
        } else {
            detected_heights[mid]
        };
        (median_h * 2.3).max(default_row_h)
    } else {
        default_row_h
    };

    // Sort candidate boxes from top to bottom
    candidate_boxes.sort_by(|a, b| {
        a.y_center
            .partial_cmp(&b.y_center)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Expand rows into vertical bands: [y_top, y_bottom]
    let mut raw_bands: Vec<(f32, f32)> = Vec::new();

    for candidate in candidate_boxes {
        let band_h = (candidate.box_height * 2.2).max(estimated_row_h);
        let shift = if candidate.is_top_line { 0.35 } else { 0.65 };

        let y_top = (candidate.y_center - band_h * shift).max(0.0);
        let y_bottom = (y_top + band_h).min(page_height);

        // Merge with previous band if overlapping or contiguous
        if let Some(last) = raw_bands.last_mut() {
            if y_top <= last.1 {
                last.1 = last.1.max(y_bottom);
                continue;
            }
        }

        raw_bands.push((y_top, y_bottom));
    }

    // Construct full-width row rectangles: (0, y_top, page_width, height)
    raw_bands
        .into_iter()
        .map(|(top, bottom)| ZeroRowRect {
            x: 0.0,
            y: top,
            width: page_width,
            height: (bottom - top).max(1.0),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_letter_o_to_zero() {
        assert_eq!(normalize_ocr_score_text("Points: O"), "Points: 0");
        assert_eq!(normalize_ocr_score_text("pts = o"), "pts: 0");
        assert_eq!(normalize_ocr_score_text("O/10"), "0/10");
        assert_eq!(normalize_ocr_score_text("o / 25"), "0/25");
    }

    #[test]
    fn detects_standard_zero_patterns() {
        let blocks = vec![
            OcrTextBlock::new("Question 1: 10/10", 50.0, 100.0, 200.0, 120.0),
            OcrTextBlock::new("Question 2: 0/10", 50.0, 200.0, 200.0, 220.0),
            OcrTextBlock::new("Total points: 10", 50.0, 300.0, 200.0, 320.0),
        ];

        let rows = detect_zero_rows(&blocks, 600.0, 800.0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].x, 0.0);
        assert_eq!(rows[0].width, 600.0);
        assert!(rows[0].y <= 200.0);
        assert!(rows[0].y + rows[0].height >= 220.0);
    }

    #[test]
    fn merges_nearby_zero_score_lines() {
        let blocks = vec![
            OcrTextBlock::new("Part A: 0", 50.0, 150.0, 120.0, 165.0),
            OcrTextBlock::new("pts: 0 / 5", 130.0, 155.0, 210.0, 170.0),
        ];

        let rows = detect_zero_rows(&blocks, 500.0, 1000.0);
        assert_eq!(rows.len(), 1, "Nearby overlapping boxes must be merged into one row");
    }
}