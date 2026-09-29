//! Groups and drawing canvases: `wpg:wgp`, `wpg:grpSp`, `wpc:wpc` and VML
//! `v:group`, read into the members of one drawing.

use docboss_model::{ChildBox, Diagnostic, Geometry, GroupFrame, GroupMember, ShapeFormat};
use docboss_xml::{Element, Ns, Reader};

use super::{css_length, css_property, int, vml_shape, xml_true, DrawingInfo, StoryParser};
use crate::xml::children;

/// The deepest nesting of groups read; deeper groups are dropped.
const MAX_DEPTH: usize = 16;
/// The most members one drawing holds.
const MAX_MEMBERS: usize = 10_000;

/// The `a:xfrm` of a group's `wpg:grpSpPr`: its offset and extent in its
/// parent's coordinates, its child offset and extent, its rotation and
/// flips.
/// ECMA-376 Part 1 §20.4.2.33, §20.1.7.5, §20.1.7.2, §20.1.7.1.
pub(super) fn group_transform(reader: &mut Reader<'_>, e: &Element<'_>, frame: &mut GroupFrame) {
    frame.rotation = e
        .attr_raw(Ns::NONE, "rot")
        .and_then(int)
        .map_or(0, |r| r.rem_euclid(21_600_000) as i32);
    frame.flip_horizontal = e.attr_raw(Ns::NONE, "flipH").is_some_and(xml_true);
    frame.flip_vertical = e.attr_raw(Ns::NONE, "flipV").is_some_and(xml_true);
    children(reader, |_, part| {
        let pair = |x: &str, y: &str| {
            let value =
                |name: &str| part.attr_raw(Ns::NONE, name).and_then(int).unwrap_or(0) as f64;
            (value(x), value(y))
        };
        match part.local {
            "off" => frame.offset = pair("x", "y"),
            "ext" => frame.extent = pair("cx", "cy"),
            "chOff" => frame.child_offset = pair("x", "y"),
            "chExt" => frame.child_extent = pair("cx", "cy"),
            _ => {}
        }
    });
}

/// A pair of numbers `x,y` as VML `coordorigin` and `coordsize` write
/// them, each defaulting to `default`.
fn vml_pair(value: Option<&str>, default: f64) -> (f64, f64) {
    let Some(value) = value else {
        return (default, default);
    };
    let mut parts = value.split(',').map(|v| {
        v.trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    });
    let x = parts.next().unwrap_or(default);
    (x, parts.next().unwrap_or(default))
}

/// The box a VML `style` gives a shape inside a group, in the group's
/// coordinate units, with its `rotation` and `flip`.
fn vml_child_box(style: &str) -> ChildBox {
    let length = |names: [&str; 2]| {
        names
            .iter()
            .find_map(|name| css_property(style, name).and_then(css_length))
            .unwrap_or(0) as f64
    };
    let flip = css_property(style, "flip").unwrap_or_default();
    ChildBox {
        x: length(["left", "margin-left"]),
        y: length(["top", "margin-top"]),
        width: length(["width", "width"]).max(0.0),
        height: length(["height", "height"]).max(0.0),
        rotation: css_property(style, "rotation")
            .and_then(|r| r.trim().parse::<f64>().ok())
            .filter(|r| r.is_finite())
            .map_or(0, |r| {
                ((r * 60_000.0).round() as i64).rem_euclid(21_600_000) as i32
            }),
        flip_horizontal: flip.contains('x'),
        flip_vertical: flip.contains('y'),
    }
}

impl StoryParser<'_> {
    /// Reads a `wpg:wgp` or `wpg:grpSp` group (ECMA-376 Part 1 §20.4.2.39,
    /// §20.4.2.32, whose `wpg:grpSpPr` is §20.4.2.33) or a `wpc:wpc`
    /// canvas (§20.4.2.41), whose children sit in its own EMU, into the
    /// members of `info`'s drawing: shapes with their text, pictures, and
    /// the members of nested groups, each placed through every group
    /// around it. `frames` holds the groups outside this one. An empty
    /// group or canvas keeps its extent as a shape with neither fill nor
    /// outline.
    pub(super) fn group(
        &mut self,
        reader: &mut Reader<'_>,
        e: &Element<'_>,
        info: &mut DrawingInfo,
        frames: &mut Vec<GroupFrame>,
    ) {
        info.drawing.geometry = None;
        if frames.len() >= MAX_DEPTH {
            self.diagnostics.push(Diagnostic::dropped(
                self.ctx.part,
                format!("a group nested deeper than {MAX_DEPTH} levels"),
            ));
            return;
        }
        let top = frames.is_empty();
        let (width, height) = (info.drawing.width as f64, info.drawing.height as f64);
        let mut frame = GroupFrame::identity(width, height);
        let canvas = e.local == "wpc";
        children(reader, |reader, child| {
            self.group_child(reader, &child, info, frames, &mut frame, canvas, top)
        });
        if top && info.drawing.members.is_empty() {
            info.drawing.geometry = Some(Box::new(Geometry::rectangle()));
            info.drawing.shape = ShapeFormat::default();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn group_child(
        &mut self,
        reader: &mut Reader<'_>,
        child: &Element<'_>,
        info: &mut DrawingInfo,
        frames: &mut Vec<GroupFrame>,
        frame: &mut GroupFrame,
        canvas: bool,
        top: bool,
    ) {
        match (child.ns, child.local) {
            (Ns::WPG, "grpSpPr") if !canvas => children(reader, |reader, x| {
                if x.ns != Ns::A || x.local != "xfrm" {
                    return;
                }
                group_transform(reader, &x, frame);
                if frame.child_extent == (0.0, 0.0) {
                    frame.child_offset = frame.offset;
                    frame.child_extent = frame.extent;
                }
                if top && info.drawing.width > 0 && info.drawing.height > 0 {
                    frame.extent = (info.drawing.width as f64, info.drawing.height as f64);
                }
            }),
            (Ns::WPS, "wsp") | (Ns::PIC, "pic") => {
                let mut member = DrawingInfo::new();
                self.drawing_children(reader, &mut member);
                let placed = member.placed.unwrap_or(ChildBox {
                    x: frame.child_offset.0,
                    y: frame.child_offset.1,
                    width: frame.child_extent.0,
                    height: frame.child_extent.1,
                    ..Default::default()
                });
                let placed = ChildBox {
                    rotation: member.drawing.shape.rotation,
                    flip_horizontal: member.drawing.shape.flip_horizontal,
                    flip_vertical: member.drawing.shape.flip_vertical,
                    ..placed
                };
                frames.push(*frame);
                let placed = GroupFrame::place_through(frames, placed);
                frames.pop();
                self.add_member(info, GroupMember::new(member.drawing, placed));
            }
            (Ns::WPG, "grpSp" | "wgp") => {
                frames.push(*frame);
                let mut nested = DrawingInfo::new();
                self.group(reader, child, &mut nested, frames);
                frames.pop();
                for member in nested.drawing.members {
                    self.add_member(info, member);
                }
            }
            (Ns::MC, "AlternateContent") => super::alternate_content(reader, |reader| {
                children(reader, |reader, inner| {
                    self.group_child(reader, &inner, info, frames, frame, canvas, top)
                })
            }),
            _ => {}
        }
    }

    pub(super) fn add_member(&mut self, info: &mut DrawingInfo, member: GroupMember) {
        if info.drawing.members.len() >= MAX_MEMBERS {
            if info.drawing.members.len() == MAX_MEMBERS {
                self.diagnostics.push(Diagnostic::dropped(
                    self.ctx.part,
                    format!("group shapes past the first {MAX_MEMBERS}"),
                ));
            }
            return;
        }
        info.drawing.members.push(member);
    }

    /// Reads a VML `v:group` (ECMA-376 Part 1 §17.3.3.19 carries VML
    /// drawings) into the members of `info`'s drawing: its `coordorigin`
    /// and `coordsize` give its children's coordinates, which their
    /// `style` positions and sizes are in; nested groups place their own
    /// children in the same way.
    pub(super) fn vml_group(
        &mut self,
        reader: &mut Reader<'_>,
        e: &Element<'_>,
        info: &mut DrawingInfo,
        frames: &mut Vec<GroupFrame>,
    ) {
        info.drawing.geometry = None;
        if frames.len() >= MAX_DEPTH {
            self.diagnostics.push(Diagnostic::dropped(
                self.ctx.part,
                format!("a group nested deeper than {MAX_DEPTH} levels"),
            ));
            return;
        }
        let style = e.attr(Ns::NONE, "style").unwrap_or_default();
        let own = match frames.is_empty() {
            true => ChildBox {
                width: info.drawing.width as f64,
                height: info.drawing.height as f64,
                ..vml_child_box(&style)
            },
            false => vml_child_box(&style),
        };
        let child_offset = vml_pair(e.attr_raw(Ns::NONE, "coordorigin"), 0.0);
        let child_extent = vml_pair(e.attr_raw(Ns::NONE, "coordsize"), 1000.0);
        let frame = GroupFrame {
            offset: (own.x, own.y),
            extent: (own.width, own.height),
            child_offset,
            child_extent,
            rotation: own.rotation,
            flip_horizontal: own.flip_horizontal,
            flip_vertical: own.flip_vertical,
        };
        frames.push(frame);
        children(reader, |reader, child| {
            self.vml_group_child(reader, &child, info, frames)
        });
        frames.pop();
    }

    fn vml_group_child(
        &mut self,
        reader: &mut Reader<'_>,
        child: &Element<'_>,
        info: &mut DrawingInfo,
        frames: &mut Vec<GroupFrame>,
    ) {
        match (child.ns, child.local) {
            (Ns::V, "group") => {
                let mut nested = DrawingInfo::new();
                self.vml_group(reader, child, &mut nested, frames);
                for member in nested.drawing.members {
                    self.add_member(info, member);
                }
            }
            (Ns::V, "shape" | "rect" | "roundrect" | "oval" | "line" | "image") => {
                let style = child.attr(Ns::NONE, "style").unwrap_or_default();
                let own = vml_child_box(&style);
                let mut member = DrawingInfo::new();
                member.drawing.width = own.width as i64;
                member.drawing.height = own.height as i64;
                member.horizontal.offset = own.x as i64;
                member.vertical.offset = own.y as i64;
                vml_shape(child, &mut member);
                if child.local == "image" {
                    member.drawing.geometry = None;
                    if let Some(id) = child
                        .attr(Ns::R, "id")
                        .or_else(|| child.attr(Ns::O, "relid"))
                    {
                        member.drawing.media = self.media_for(&id);
                    }
                }
                self.vml(reader, &mut member);
                let placed = ChildBox {
                    x: member.horizontal.offset as f64,
                    y: member.vertical.offset as f64,
                    width: member.drawing.width as f64,
                    height: member.drawing.height as f64,
                    rotation: member.drawing.shape.rotation,
                    flip_horizontal: member.drawing.shape.flip_horizontal,
                    flip_vertical: member.drawing.shape.flip_vertical,
                };
                let placed = GroupFrame::place_through(frames, placed);
                let drawn = member.drawing.media.is_some()
                    || !member.drawing.text_box.is_empty()
                    || member.drawing.geometry.is_some();
                if drawn {
                    self.add_member(info, GroupMember::new(member.drawing, placed));
                }
            }
            _ => {}
        }
    }
}
