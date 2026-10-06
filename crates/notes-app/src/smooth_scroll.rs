//! Vertical scroller with browser-like wheel motion: short smooth catch-up,
//! not an inertial coast.

use cosmic::iced::core::renderer::Renderer as _;
use cosmic::iced::core::time::{Duration, Instant};
use cosmic::iced::core::widget::{tree, Tree, Widget};
use cosmic::iced::core::window;
use cosmic::iced::core::{
    layout, mouse, renderer, Clipboard, Color, Event, Layout, Length, Rectangle, Shell, Size,
    Vector,
};
use cosmic::iced::{Background, Border};
use cosmic::Element;

const LINE_PX: f32 = 112.0;
const PIXEL_GAIN: f32 = 1.55;
/// Time constant for the last notch (~three frames to most of the way).
const TAU_S: f32 = 0.05;
/// Pending ease is at most about one wheel notch, so bursts cannot coast.
const MAX_LAG: f32 = LINE_PX * 1.25;
const SNAP_PX: f32 = 0.35;
const FRAME_MS: u64 = 16;
const RAIL_W: f32 = 8.0;
const RAIL_PAD: f32 = 4.0;

fn follow(offset: f32, target: f32, dt: f32) -> f32 {
    let dt = dt.clamp(0.0, 0.05);
    let lag = target - offset;
    let offset = if lag.abs() > MAX_LAG {
        target - lag.signum() * MAX_LAG
    } else {
        offset
    };
    let k = 1.0 - (-dt / TAU_S).exp();
    let next = offset + (target - offset) * k;
    if (target - next).abs() <= SNAP_PX {
        target
    } else {
        next
    }
}

pub fn vertical<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    height: Length,
) -> Element<'a, Message> {
    Element::new(SmoothScroll {
        content: content.into(),
        height,
    })
}

struct SmoothScroll<'a, Message> {
    content: Element<'a, Message>,
    height: Length,
}

struct State {
    offset: f32,
    target: f32,
    content_h: f32,
    last: Instant,
    dragging: Option<f32>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            offset: 0.0,
            target: 0.0,
            content_h: 0.0,
            last: Instant::now(),
            dragging: None,
        }
    }
}

impl State {
    fn max_offset(&self, viewport_h: f32) -> f32 {
        (self.content_h - viewport_h).max(0.0)
    }

    fn clamp(&mut self, viewport_h: f32) {
        let max = self.max_offset(viewport_h);
        self.target = self.target.clamp(0.0, max);
        self.offset = self.offset.clamp(0.0, max);
    }
}

fn thumb_rect(bounds: Rectangle, offset: f32, content_h: f32) -> Option<Rectangle> {
    let max = (content_h - bounds.height).max(0.0);
    if max <= 0.5 {
        return None;
    }
    let track_h = bounds.height;
    let thumb_h = (bounds.height / content_h * track_h).clamp(24.0, track_h);
    let travel = (track_h - thumb_h).max(0.0);
    let y = bounds.y + if max > 0.0 { offset / max * travel } else { 0.0 };
    Some(Rectangle {
        x: bounds.x + bounds.width - RAIL_W - RAIL_PAD,
        y,
        width: RAIL_W,
        height: thumb_h,
    })
}

impl<'a, Message> Widget<Message, cosmic::Theme, cosmic::Renderer> for SmoothScroll<'a, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, self.height)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let limits = limits.width(Length::Fill).height(self.height);
        let size = limits.max();
        let inner_w = (size.width - RAIL_W - RAIL_PAD * 2.0).max(1.0);
        let child = self.content.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(
                Size::new(0.0, size.height),
                Size::new(inner_w, f32::INFINITY),
            ),
        );
        let state = tree.state.downcast_mut::<State>();
        state.content_h = child.size().height;
        state.clamp(size.height);
        layout::Node::with_children(size, vec![child])
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &cosmic::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let content_layout = layout.children().next().unwrap();
        let state = tree.state.downcast_mut::<State>();
        let now = match event {
            Event::Window(window::Event::RedrawRequested(now)) => *now,
            _ => Instant::now(),
        };
        if state.dragging.is_none() {
            let dt = now.saturating_duration_since(state.last).as_secs_f32();
            state.last = now;
            state.offset = follow(state.offset, state.target, dt);
            state.clamp(bounds.height);
            if (state.target - state.offset).abs() > SNAP_PX {
                shell.request_redraw_at(now + Duration::from_millis(FRAME_MS));
            } else {
                state.offset = state.target;
            }
        } else {
            state.clamp(bounds.height);
        }
        let over = cursor.is_over(bounds);
        let translation = Vector::new(0.0, state.offset);
        let thumb = thumb_rect(bounds, state.offset, state.content_h);

        match event {
            Event::Window(window::Event::RedrawRequested(_)) => {}
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if over => {
                let dy = match *delta {
                    mouse::ScrollDelta::Lines { y, .. } => -y * LINE_PX,
                    mouse::ScrollDelta::Pixels { y, .. } => -y * PIXEL_GAIN,
                };
                state.target = (state.target + dy).clamp(0.0, state.max_offset(bounds.height));
                state.last = Instant::now();
                shell.request_redraw();
                shell.capture_event();
                return;
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                if let (Some(thumb), Some(pos)) = (thumb, cursor.position()) {
                    if thumb.contains(pos) {
                        state.dragging = Some(pos.y - thumb.y);
                        shell.capture_event();
                        return;
                    }
                    if pos.x >= thumb.x {
                        let max = state.max_offset(bounds.height);
                        let ratio = ((pos.y - bounds.y) / bounds.height).clamp(0.0, 1.0);
                        state.target = ratio * max;
                        state.last = Instant::now();
                        shell.request_redraw();
                        shell.capture_event();
                        return;
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging.take().is_some() {
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let (Some(grab), Some(pos), Some(thumb)) =
                    (state.dragging, cursor.position(), thumb)
                {
                    let max = state.max_offset(bounds.height);
                    let travel = (bounds.height - thumb.height).max(1.0);
                    let ratio = ((pos.y - grab - bounds.y) / travel).clamp(0.0, 1.0);
                    state.target = ratio * max;
                    state.offset = state.target;
                    shell.request_redraw();
                    shell.capture_event();
                    return;
                }
            }
            _ => {}
        }

        let cursor = if over && state.dragging.is_none() {
            cursor
                .position()
                .map(|p| mouse::Cursor::Available(p + translation))
                .unwrap_or(cursor)
        } else {
            mouse::Cursor::Unavailable
        };
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            content_layout.with_virtual_offset(translation + layout.virtual_offset()),
            cursor,
            renderer,
            clipboard,
            shell,
            &Rectangle {
                y: bounds.y + translation.y,
                x: bounds.x,
                ..bounds
            },
        );
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut cosmic::Renderer,
        theme: &cosmic::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let Some(visible) = bounds.intersection(viewport) else {
            return;
        };
        let content_layout = layout.children().next().unwrap();
        let translation = Vector::new(0.0, state.offset);
        let over = cursor.is_over(bounds);
        let child_cursor = if over {
            cursor
                .position()
                .map(|p| mouse::Cursor::Available(p + translation))
                .unwrap_or(cursor)
        } else {
            mouse::Cursor::Unavailable
        };

        renderer.with_layer(visible, |renderer| {
            renderer.with_translation(Vector::new(0.0, -state.offset), |renderer| {
                self.content.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    content_layout.with_virtual_offset(translation + layout.virtual_offset()),
                    child_cursor,
                    &Rectangle {
                        y: visible.y + translation.y,
                        x: visible.x,
                        ..visible
                    },
                );
            });
        });

        if let Some(thumb) = thumb_rect(bounds, state.offset, state.content_h) {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: thumb,
                    border: Border {
                        radius: 4.0.into(),
                        ..Border::default()
                    },
                    ..renderer::Quad::default()
                },
                Background::Color(Color {
                    a: 0.35,
                    ..style.text_color
                }),
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &cosmic::Renderer,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();
        if !cursor.is_over(bounds) {
            return mouse::Interaction::default();
        }
        let state = tree.state.downcast_ref::<State>();
        if state.dragging.is_some()
            || thumb_rect(bounds, state.offset, state.content_h)
                .is_some_and(|thumb| cursor.is_over(thumb))
        {
            return mouse::Interaction::Pointer;
        }
        let translation = Vector::new(0.0, state.offset);
        let content_layout = layout.children().next().unwrap();
        let child_cursor = cursor
            .position()
            .map(|p| mouse::Cursor::Available(p + translation))
            .unwrap_or(cursor);
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            content_layout.with_virtual_offset(layout.virtual_offset()),
            child_cursor,
            viewport,
            renderer,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{follow, LINE_PX, MAX_LAG};

    #[test]
    fn follow_stays_between_offset_and_target() {
        let next = follow(10.0, 80.0, 0.016);
        assert!(next > 10.0 && next <= 80.0);
    }

    #[test]
    fn follow_caps_lag_so_a_burst_cannot_coast() {
        let next = follow(0.0, 800.0, 0.0);
        assert!((next - (800.0 - MAX_LAG)).abs() < 0.01);
    }

    #[test]
    fn follow_settles_the_last_notch_quickly() {
        let mut offset = 0.0;
        let target = LINE_PX;
        for _ in 0..12 {
            offset = follow(offset, target, 0.016);
        }
        assert!(
            (target - offset).abs() < 8.0,
            "still {offset} after ~192ms, target {target}"
        );
    }
}
