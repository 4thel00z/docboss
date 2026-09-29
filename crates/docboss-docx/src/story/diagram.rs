//! Diagrams (SmartArt): the laid-out drawing Office saves beside a
//! diagram's data model, read into the members of one drawing, and the
//! data model's text in reading order.

use std::collections::HashMap;

use docboss_model::{Block, ChildBox, Diagnostic, GroupFrame, GroupMember, ShapeFormat};
use docboss_xml::{decode, Element, Ns, Reader};

use super::text::{text_body, TextDefaults};
use super::{shape_properties, shape_style, Context, DrawingInfo, StoryParser};
use crate::xml::{children, int, root};

/// The deepest nesting of groups read in a diagram drawing.
const MAX_DEPTH: usize = 16;
/// The most points of a data model read.
const MAX_POINTS: usize = 10_000;

/// A point of a diagram's data model with text.
struct Point {
    id: String,
    kind: String,
    text: Vec<Block>,
}

/// What a data model part holds for reading: its points, its parent to
/// child connections in order, and the relationship id of its drawing.
#[derive(Default)]
struct DataModel {
    points: Vec<Point>,
    children: HashMap<String, Vec<(i64, String)>>,
    drawing: Option<String>,
}

impl DataModel {
    /// The text of the points that carry it, walking the connections
    /// depth first from the document point, children by their source
    /// order; points no connection reaches follow in list order.
    fn text(self) -> Vec<Block> {
        let index: HashMap<&str, usize> = self
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.as_str(), i))
            .collect();
        let mut order = Vec::new();
        let mut seen = vec![false; self.points.len()];
        let mut stack: Vec<&str> = self
            .points
            .iter()
            .filter(|p| p.kind == "doc")
            .map(|p| p.id.as_str())
            .rev()
            .collect();
        while let Some(id) = stack.pop() {
            let Some(&at) = index.get(id) else {
                continue;
            };
            if std::mem::replace(&mut seen[at], true) {
                continue;
            }
            order.push(at);
            let Some(kids) = self.children.get(id) else {
                continue;
            };
            stack.extend(kids.iter().rev().map(|(_, id)| id.as_str()));
        }
        order.extend((0..self.points.len()).filter(|&i| !seen[i]));
        let mut points: Vec<Option<Point>> = self.points.into_iter().map(Some).collect();
        order
            .into_iter()
            .filter_map(|i| points.get_mut(i).and_then(Option::take))
            .filter(|p| matches!(p.kind.as_str(), "node" | "asst"))
            .flat_map(|p| p.text)
            .collect()
    }
}

/// ECMA-376 Part 1 §21.4.2.10 `dataModel`: the points of §21.4.3.6
/// `ptLst` (§21.4.3.5 `pt`, whose `type` defaults to `node`) with their
/// §21.4.3.8 `t` text bodies, and the §21.4.3.2 `cxn` connections of
/// §21.4.3.3 `cxnLst`, whose `type` defaults to `parOf`. The drawing's
/// relationship id comes from the `dsp:dataModelExt` extension.
fn data_model(text: &str, theme: &crate::props::Theme) -> DataModel {
    let mut reader = Reader::new(text);
    let mut model = DataModel::default();
    if root(&mut reader).is_none() {
        return model;
    }
    let defaults = TextDefaults::minor(theme);
    children(&mut reader, |reader, list| match (list.ns, list.local) {
        (Ns::DGM, "ptLst") => children(reader, |reader, pt| {
            if pt.local != "pt" || model.points.len() >= MAX_POINTS {
                return;
            }
            let id = pt
                .attr(Ns::NONE, "modelId")
                .unwrap_or_default()
                .into_owned();
            let kind = pt
                .attr(Ns::NONE, "type")
                .map_or("node".into(), |t| t.into_owned());
            let mut text = Vec::new();
            children(reader, |reader, part| {
                if part.ns == Ns::DGM && part.local == "t" {
                    text = text_body(reader, theme, &defaults, None, |_, _| {});
                }
            });
            text.retain(
                |block| !matches!(block, Block::Paragraph(p) if p.text().trim().is_empty()),
            );
            model.points.push(Point { id, kind, text });
        }),
        (Ns::DGM, "cxnLst") => children(reader, |_, cxn| {
            if cxn.local != "cxn" {
                return;
            }
            let kind = cxn.attr_raw(Ns::NONE, "type").unwrap_or("parOf");
            if kind != "parOf" {
                return;
            }
            let (Some(source), Some(dest)) =
                (cxn.attr(Ns::NONE, "srcId"), cxn.attr(Ns::NONE, "destId"))
            else {
                return;
            };
            let order = cxn.attr_raw(Ns::NONE, "srcOrd").and_then(int).unwrap_or(0);
            let list = model.children.entry(source.into_owned()).or_default();
            if list.len() < MAX_POINTS {
                list.push((order, dest.into_owned()));
            }
        }),
        (Ns::DGM, "extLst") => find_drawing(reader, &mut model.drawing),
        _ => {}
    });
    for list in model.children.values_mut() {
        list.sort_by_key(|(order, _)| *order);
    }
    model
}

fn find_drawing(reader: &mut Reader<'_>, found: &mut Option<String>) {
    children(reader, |reader, e| {
        if e.ns == Ns::DSP && e.local == "dataModelExt" {
            *found = e.attr(Ns::NONE, "relId").map(Into::into);
            return;
        }
        find_drawing(reader, found);
    });
}

impl StoryParser<'_> {
    /// Reads a diagram from `dgm:relIds` (ECMA-376 Part 1 §21.4.2.22): its
    /// data model gives the drawing's data text, and the drawing Office
    /// saved of the laid-out diagram gives its shapes, which become the
    /// drawing's members. The layout, style and color parts are not
    /// evaluated. Returns false when the diagram has no saved drawing.
    pub(super) fn diagram(&mut self, e: &Element<'_>, info: &mut DrawingInfo) -> bool {
        info.drawing.geometry = None;
        let data = e
            .attr(Ns::R, "dm")
            .and_then(|id| self.ctx.rels.get(&id))
            .filter(|rel| !rel.external)
            .map(|rel| rel.target.clone());
        let Some(data) = data else {
            self.diagnostics.push(Diagnostic::dropped(
                self.ctx.part,
                "a diagram without a data model part",
            ));
            return false;
        };
        let Some(bytes) = self.ctx.package.part_into(&data, &mut self.diagnostics) else {
            self.diagnostics.push(Diagnostic::dropped(
                data.as_str(),
                "the diagram data part is missing",
            ));
            return false;
        };
        let model = data_model(&decode(&bytes), self.ctx.theme);
        let drawing = model
            .drawing
            .as_deref()
            .and_then(|id| self.ctx.rels.get(id))
            .filter(|rel| !rel.external && self.ctx.package.archive.contains(&rel.target))
            .map(|rel| rel.target.clone());
        info.drawing.data_text = model.text();
        let Some(drawing) = drawing else {
            self.diagnostics.push(Diagnostic::dropped(
                self.ctx.part,
                "a diagram without its saved drawing is not drawn",
            ));
            return false;
        };
        self.diagram_drawing(&drawing, info);
        true
    }

    fn diagram_drawing(&mut self, name: &str, info: &mut DrawingInfo) {
        let Some(bytes) = self.ctx.package.part_into(name, &mut self.diagnostics) else {
            return;
        };
        let rels = self.ctx.package.relationships(name, &mut self.diagnostics);
        let text = decode(&bytes);
        let mut reader = Reader::new(&text);
        if root(&mut reader).is_none() {
            return;
        }
        let mut nested = StoryParser::new(Context {
            part: name,
            rels: &rels,
            ..self.ctx
        });
        let frame = GroupFrame::identity(info.drawing.width as f64, info.drawing.height as f64);
        children(&mut reader, |reader, e| {
            if e.ns == Ns::DSP && e.local == "spTree" {
                nested.diagram_group(reader, info, &mut Vec::new(), frame);
            }
        });
        self.diagnostics.append(&mut nested.diagnostics);
    }

    /// A `dsp:spTree` or `dsp:grpSp`: its `dsp:grpSpPr` places its
    /// children as a `wpg:grpSp` does (ECMA-376 Part 1 §20.1.7.5), and each
    /// `dsp:sp` is a shape like `wps:wsp`.
    fn diagram_group(
        &mut self,
        reader: &mut Reader<'_>,
        info: &mut DrawingInfo,
        frames: &mut Vec<GroupFrame>,
        mut frame: GroupFrame,
    ) {
        if frames.len() >= MAX_DEPTH {
            self.diagnostics.push(Diagnostic::dropped(
                self.ctx.part,
                format!("a diagram group nested deeper than {MAX_DEPTH} levels"),
            ));
            return;
        }
        let top = frames.is_empty();
        children(reader, |reader, child| match (child.ns, child.local) {
            (Ns::DSP, "grpSpPr") => children(reader, |reader, x| {
                if x.ns != Ns::A || x.local != "xfrm" {
                    return;
                }
                let own = frame;
                super::group::group_transform(reader, &x, &mut frame);
                if frame.child_extent == (0.0, 0.0) {
                    frame.child_offset = frame.offset;
                    frame.child_extent = frame.extent;
                }
                if top {
                    frame.offset = own.offset;
                    frame.extent = own.extent;
                    if frame.child_extent.0 <= 0.0 || frame.child_extent.1 <= 0.0 {
                        frame.child_offset = (0.0, 0.0);
                        frame.child_extent = own.extent;
                    }
                }
            }),
            (Ns::DSP, "sp") => self.diagram_shape(reader, info, frames, frame),
            (Ns::DSP, "grpSp") => {
                frames.push(frame);
                let inner = GroupFrame::identity(0.0, 0.0);
                self.diagram_group(reader, info, frames, inner);
                frames.pop();
            }
            _ => {}
        });
    }

    /// A `dsp:sp`: `dsp:spPr` and `dsp:style` as `wps:spPr` and
    /// `wps:style` give them, the `dsp:txBody` DrawingML text in the
    /// style's font and color, and `dsp:txXfrm`, the text's own box, which
    /// becomes a second member without fill or outline. The text box's
    /// turn adds to the shape's, as LibreOffice reads it.
    fn diagram_shape(
        &mut self,
        reader: &mut Reader<'_>,
        info: &mut DrawingInfo,
        frames: &mut Vec<GroupFrame>,
        frame: GroupFrame,
    ) {
        let mut shape = DrawingInfo::new();
        let mut defaults = TextDefaults::minor(self.ctx.theme);
        let mut text = DrawingInfo::new();
        let mut text_box: Option<ChildBox> = None;
        children(reader, |reader, e| match (e.ns, e.local) {
            (Ns::DSP, "spPr") => {
                if let Some(note) = shape_properties(reader, self.ctx.theme, &mut shape) {
                    self.diagnostics
                        .push(Diagnostic::approximated(self.ctx.part, note));
                }
                self.fill_picture(&mut shape);
            }
            (Ns::DSP, "style") => {
                let (face, color) = shape_style(reader, self.ctx.theme, &mut shape);
                if let Some(face) = face {
                    defaults.font = Some(face);
                }
                defaults.color = color.or(defaults.color);
            }
            (Ns::DSP, "txBody") => {
                let theme = self.ctx.theme;
                text.drawing.text_box =
                    text_body(reader, theme, &defaults, None, |reader, body| {
                        if body.ns == Ns::A && body.local == "bodyPr" {
                            self.body_properties(reader, body, &mut text);
                        }
                    });
            }
            (Ns::DSP, "txXfrm") => {
                let mut boxed = DrawingInfo::new();
                super::transform_2d(reader, &e, &mut boxed);
                let turn = i64::from(shape.drawing.shape.rotation)
                    + i64::from(boxed.drawing.shape.rotation);
                text_box = boxed.placed.map(|placed| ChildBox {
                    rotation: turn.rem_euclid(21_600_000) as i32,
                    ..placed
                });
            }
            _ => {}
        });
        let own = shape.placed.unwrap_or(ChildBox {
            x: frame.child_offset.0,
            y: frame.child_offset.1,
            width: frame.child_extent.0,
            height: frame.child_extent.1,
            ..Default::default()
        });
        let own = ChildBox {
            rotation: shape.drawing.shape.rotation,
            flip_horizontal: shape.drawing.shape.flip_horizontal,
            flip_vertical: shape.drawing.shape.flip_vertical,
            ..own
        };
        let text_shape = text.drawing.shape;
        let format = &mut shape.drawing.shape;
        format.insets = text_shape.insets;
        format.text_anchor = text_shape.text_anchor;
        format.text_direction = text_shape.text_direction;
        format.text_upright = text_shape.text_upright;
        let has_text = text
            .drawing
            .text_box
            .iter()
            .any(|block| !matches!(block, Block::Paragraph(p) if p.inlines.is_empty()));
        frames.push(frame);
        match text_box.filter(|_| has_text) {
            Some(text_box) => {
                let placed = GroupFrame::place_through(frames, own);
                self.add_member(info, GroupMember::new(shape.drawing, placed));
                text.drawing.shape = ShapeFormat {
                    insets: text_shape.insets,
                    text_anchor: text_shape.text_anchor,
                    text_direction: text_shape.text_direction,
                    text_upright: text_shape.text_upright,
                    ..ShapeFormat::default()
                };
                let placed = GroupFrame::place_through(frames, text_box);
                self.add_member(info, GroupMember::new(text.drawing, placed));
            }
            None => {
                if has_text {
                    shape.drawing.text_box = text.drawing.text_box;
                }
                let placed = GroupFrame::place_through(frames, own);
                self.add_member(info, GroupMember::new(shape.drawing, placed));
            }
        }
        frames.pop();
    }
}
