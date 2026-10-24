//! Check the stroke semantics interpreted from actual exported PDF paths.
use super::*;
use hayro_interpret::font::Glyph;
use hayro_interpret::{
    BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image as InterpretedImage, InterpreterCache, InterpreterSettings, Paint, PathDrawMode,
    SoftMask, StrokeProps, interpret_page,
};
use wordcraft_doc::para::{Anchor, Float, Wrap};
use wordcraft_doc::{CharProps, Paragraph, para_block};

struct Strokes(Vec<StrokeProps>);
impl Device<'_> for Strokes {
    fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
    fn set_blend_mode(&mut self, _: BlendMode) {}
    fn draw_path(&mut self, _: &kurbo::BezPath, _: kurbo::Affine, _: &Paint<'_>, mode: &PathDrawMode) {
        if let PathDrawMode::Stroke(stroke) = mode {
            self.0.push(stroke.clone());
        }
    }
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
    fn draw_glyph(&mut self, _: &Glyph<'_>, _: kurbo::Affine, _: kurbo::Affine, _: &Paint<'_>, _: &GlyphDrawMode) {}
    fn draw_image(&mut self, _: InterpretedImage<'_, '_>, _: kurbo::Affine) {}
    fn pop_clip_path(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

#[test]
fn shape_outlines_export_round_caps_and_joins_at_native_widths() {
    for kind in [ShapeKind::Rectangle, ShapeKind::Ellipse, ShapeKind::Line] {
        for width in [0.25, 2.0, 100.0, 200.0] {
            let mut document = Document::new();
            let mut paragraph = Paragraph::new();
            paragraph
                .insert_object(
                    0,
                    InlineObject::Shape {
                        kind,
                        w: 160.0,
                        h: 100.0,
                        fill: None,
                        stroke: Some(Rgb(0, 0, 255)),
                        stroke_width: width,
                        float: Float { wrap: Wrap::BehindText, h_rel: Anchor::Page, v_rel: Anchor::Page, x: 140.0, y: 140.0, ..Default::default() },
                        story: None,
                        freeform: None,
                        effects: Default::default(),
                        extra: Default::default(),
                    },
                    &CharProps::default(),
                )
                .unwrap();
            document.body = vec![para_block(paragraph)];
            let bytes = export(&document, &PdfOptions::default()).unwrap();
            let pdf = hayro_syntax::Pdf::new(bytes).unwrap();
            assert_eq!(pdf.pages().len(), 1);
            let cache = InterpreterCache::new();
            let mut context =
                Context::new(kurbo::Affine::IDENTITY, kurbo::Rect::new(0.0, 0.0, 1.0, 1.0), &cache, pdf.xref(), InterpreterSettings::default());
            let mut strokes = Strokes(Vec::new());
            interpret_page(&pdf.pages()[0], &mut context, &mut strokes);
            assert_eq!(strokes.0.len(), 1, "{kind:?}/{width}");
            let stroke = &strokes.0[0];
            assert_eq!(stroke.line_cap, kurbo::Cap::Round, "{kind:?}/{width}");
            assert_eq!(stroke.line_join, kurbo::Join::Round, "{kind:?}/{width}");
            assert_eq!(stroke.line_width, width, "{kind:?}/{width}");
            assert!(stroke.dash_array.is_empty());
        }
    }
}
