//! PDF Highlight annotation builder and appearance stream generator.
//! Implements ISO 32000-1 §12.5.6.10 Markup Annotations (`/Subtype /Highlight`).
//! Generates vector rectangle paths and transparency graphics states.

use std::fmt;

/// An RGBA floating-point color with channels in range 0.0 ..= 1.0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RgbaColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl RgbaColor {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Default grading highlighter orange (RGBA: 1.0, 0.55, 0.0, 0.35).
    pub const fn grading_orange() -> Self {
        Self::new(1.0, 0.55, 0.0, 0.35)
    }

    /// Formats as PDF non-stroking color operator (e.g. `1.0 0.55 0.0 rg`).
    pub fn to_pdf_rg(&self) -> String {
        format!("{:.3} {:.3} {:.3} rg", self.r, self.g, self.b)
    }
}

/// Visual style configuration for a highlight markup annotation.
#[derive(Debug, Clone, PartialEq)]
pub struct HighlightStyle {
    pub color: RgbaColor,
    pub author: String,
    pub subject: String,
}

impl Default for HighlightStyle {
    fn default() -> Self {
        Self {
            color: RgbaColor::grading_orange(),
            author: "PdfCraft".to_string(),
            subject: "Highlight".to_string(),
        }
    }
}

/// Low-level representation of an annotation object dictionary ready for insertion into PDF COS.
#[derive(Debug, Clone, PartialEq)]
pub struct RawAnnotationObject {
    pub subtype: &'static str,
    pub rect: [f32; 4],
    pub quad_points: Vec<f32>,
    pub color_rgb: [f32; 3],
    pub opacity: f32,
    pub contents: String,
    pub author: String,
    pub subject: String,
    pub appearance_stream_bytes: Vec<u8>,
}

/// Strongly-typed Highlight Annotation complying with ISO 32000-1.
#[derive(Debug, Clone, PartialEq)]
pub struct HighlightAnnotation {
    pub rect: [f32; 4],
    pub quad_points: [f32; 8],
    pub style: HighlightStyle,
    pub contents: String,
}

impl HighlightAnnotation {
    /// Constructs raw COS dictionaries and appearance stream content.
    pub fn into_raw_annotation(self) -> RawAnnotationObject {
        let stream = self.generate_appearance_stream();
        RawAnnotationObject {
            subtype: "Highlight",
            rect: self.rect,
            quad_points: self.quad_points.to_vec(),
            color_rgb: [self.style.color.r, self.style.color.g, self.style.color.b],
            opacity: self.style.color.a,
            contents: self.contents,
            author: self.style.author,
            subject: self.style.subject,
            appearance_stream_bytes: stream.into_bytes(),
        }
    }

    /// Emits PDF graphics operators drawing the semi-transparent highlight box.
    pub fn generate_appearance_stream(&self) -> String {
        let [x1, y1, x2, y2] = self.rect;
        let w = (x2 - x1).abs();
        let h = (y2 - y1).abs();
        let min_x = x1.min(x2);
        let min_y = y1.min(y2);

        // PDF operators:
        // q: push graphics state
        // /GS0 gs: apply blend mode and alpha
        // r g b rg: set non-stroking color
        // x y w h re: append rectangle
        // f: fill path
        // Q: pop graphics state
        format!(
            "q\n/GS0 gs\n{color_rg}\n{x:.2} {y:.2} {w:.2} {h:.2} re\nf\nQ\n",
            color_rg = self.style.color.to_pdf_rg(),
            x = min_x,
            y = min_y,
            w = w,
            h = h,
        )
    }
}

/// Fluent builder for constructing `HighlightAnnotation` instances.
#[derive(Default)]
pub struct HighlightBuilder {
    x: Option<f32>,
    y: Option<f32>,
    width: Option<f32>,
    height: Option<f32>,
    style: Option<HighlightStyle>,
    contents: Option<String>,
}

#[derive(Debug)]
pub enum BuilderError {
    MissingDimensions,
}

impl fmt::Display for BuilderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDimensions => write!(f, "Highlight rectangle x, y, width, and height are required"),
        }
    }
}

impl std::error::Error for BuilderError {}

impl HighlightBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn rect(mut self, x: f32, y: f32, width: f32, height: f32) -> Self {
        self.x = Some(x);
        self.y = Some(y);
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    pub fn style(mut self, style: HighlightStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn contents(mut self, contents: impl Into<String>) -> Self {
        self.contents = Some(contents.into());
        self
    }

    pub fn build(self) -> Result<HighlightAnnotation, BuilderError> {
        let x = self.x.ok_or(BuilderError::MissingDimensions)?;
        let y = self.y.ok_or(BuilderError::MissingDimensions)?;
        let w = self.width.ok_or(BuilderError::MissingDimensions)?;
        let h = self.height.ok_or(BuilderError::MissingDimensions)?;

        let x1 = x;
        let y1 = y;
        let x2 = x + w;
        let y2 = y + h;

        // QuadPoints format per ISO 32000: x1 y1 x2 y2 x3 y3 x4 y4
        // (top-left, top-right, bottom-left, bottom-right)
        let quad_points = [x1, y2, x2, y2, x1, y1, x2, y1];

        Ok(HighlightAnnotation {
            rect: [x1, y1, x2, y2],
            quad_points,
            style: self.style.unwrap_or_default(),
            contents: self.contents.unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_highlight_appearance_stream() {
        let annot = HighlightBuilder::new()
            .rect(0.0, 100.0, 500.0, 40.0)
            .style(HighlightStyle {
                color: RgbaColor::new(1.0, 0.55, 0.0, 0.4),
                author: "Test".into(),
                subject: "Zero".into(),
            })
            .contents("Row 0 Score")
            .build()
            .unwrap();

        let stream = annot.generate_appearance_stream();
        assert!(stream.contains("1.000 0.550 0.000 rg"));
        assert!(stream.contains("0.00 100.00 500.00 40.00 re"));
        assert!(stream.contains("f\nQ"));

        let raw = annot.into_raw_annotation();
        assert_eq!(raw.subtype, "Highlight");
        assert_eq!(raw.rect, [0.0, 100.0, 500.0, 140.0]);
        assert_eq!(raw.quad_points.len(), 8);
    }
}