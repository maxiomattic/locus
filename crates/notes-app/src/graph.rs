//! Note graph from `[[wikilinks]]`. Click a node to open it; missing targets
//! are hollow nodes that create the page. Layout is a live force simulation.

use cosmic::iced::alignment;
use cosmic::iced::core::time::Instant;
use cosmic::iced::mouse;
use cosmic::iced::widget::canvas::{self, Canvas, Fill, Frame, Geometry, Path, Stroke, Text};
use cosmic::iced::window;
use cosmic::iced::{Color, Event, Length, Pixels, Point, Rectangle, Renderer, Size, Vector};
use cosmic::Element;
use notes_sync::Note;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::hash::{Hash, Hasher};

use crate::markdown;
use crate::Message;

const CHARGE: f32 = 420.0;
const LINK_DIST: f32 = 96.0;
const LINK_K: f32 = 0.08;
const GRAVITY: f32 = 0.012;
const DAMPING: f32 = 0.72;
const MAX_SPEED: f32 = 28.0;
const SETTLE_SPEED: f32 = 0.35;
const SETTLE_FRAMES: u8 = 14;
const DRAG_CLICK_PX: f32 = 5.0;
const ZOOM_MIN: f32 = 0.12;
const ZOOM_MAX: f32 = 6.0;
const REPEL_CELL: f32 = 140.0;

#[derive(Clone, Debug)]
pub enum NodeKind {
    Note(u64),
    Missing(String),
}

#[derive(Clone, Debug)]
pub struct GraphNode {
    pub kind: NodeKind,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<(usize, usize)>,
}

pub fn build(notes: &HashMap<u64, Note>) -> GraphData {
    let mut nodes: Vec<GraphNode> = notes
        .values()
        .map(|n| GraphNode {
            kind: NodeKind::Note(n.id),
            label: n.title.clone(),
        })
        .collect();
    nodes.sort_by(|a, b| match (&a.kind, &b.kind) {
        (NodeKind::Note(x), NodeKind::Note(y)) => y.cmp(x),
        _ => std::cmp::Ordering::Equal,
    });

    let mut index: HashMap<String, usize> = HashMap::new();
    for (i, node) in nodes.iter().enumerate() {
        index.entry(normalize(&node.label)).or_insert(i);
    }
    for (i, node) in nodes.iter().enumerate() {
        let key = normalize(&node.label);
        let base = basename(&key);
        if base != key {
            index.entry(base.to_string()).or_insert(i);
        }
    }

    let id_at: HashMap<u64, usize> = nodes
        .iter()
        .enumerate()
        .filter_map(|(i, node)| match node.kind {
            NodeKind::Note(id) => Some((id, i)),
            NodeKind::Missing(_) => None,
        })
        .collect();

    let mut edges = Vec::new();
    let mut missing_at: HashMap<String, usize> = HashMap::new();
    for note in notes.values() {
        let Some(&from) = id_at.get(&note.id) else {
            continue;
        };
        for link in markdown::wikilinks(&note.body_md) {
            let key = normalize(&link.target);
            if let Some(&to) = resolve(&index, &key) {
                if from != to {
                    edges.push(ordered(from, to));
                }
            } else {
                let to = if let Some(&t) = missing_at.get(&key) {
                    t
                } else {
                    let t = nodes.len();
                    nodes.push(GraphNode {
                        kind: NodeKind::Missing(link.target.clone()),
                        label: link.target.clone(),
                    });
                    missing_at.insert(key, t);
                    t
                };
                edges.push(ordered(from, to));
            }
        }
    }
    edges.sort_unstable();
    edges.dedup();
    GraphData { nodes, edges }
}

pub fn view<'a>(data: GraphData, selected: Option<u64>) -> Element<'a, Message> {
    Canvas::new(GraphPlot { data, selected })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

struct GraphPlot {
    data: GraphData,
    selected: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
struct Body {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum NodeKey {
    Note(u64),
    Missing(String),
}

#[derive(Clone, Copy, Debug)]
struct Camera {
    pan: Vector,
    zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            pan: Vector::ZERO,
            zoom: 1.0,
        }
    }
}

enum Drag {
    None,
    Node { index: usize, grab: Vector },
    Pan { last: Point },
}

struct PlotState {
    keys: Vec<NodeKey>,
    bodies: Vec<Body>,
    edges: Vec<(usize, usize)>,
    camera: Camera,
    camera_ready: bool,
    drag: Drag,
    press_start: Option<Point>,
    dragged: bool,
    hover: Option<usize>,
    last: Instant,
    topology: u64,
    settled: bool,
    quiet: u8,
}

impl Default for PlotState {
    fn default() -> Self {
        Self {
            keys: Vec::new(),
            bodies: Vec::new(),
            edges: Vec::new(),
            camera: Camera::default(),
            camera_ready: false,
            drag: Drag::None,
            press_start: None,
            dragged: false,
            hover: None,
            last: Instant::now(),
            topology: 0,
            settled: true,
            quiet: 0,
        }
    }
}

impl canvas::Program<Message, cosmic::Theme> for GraphPlot {
    type State = PlotState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        sync(state, &self.data, bounds.size());
        let over = cursor.position_in(bounds);
        let hit = over.and_then(|pos| hit_index(state, pos, self.data.nodes.len()));

        match event {
            Event::Window(window::Event::RedrawRequested(now)) => {
                let dt = now.saturating_duration_since(state.last).as_secs_f32();
                state.last = *now;
                let pinned = match state.drag {
                    Drag::Node { index, .. } => Some(index),
                    _ => None,
                };
                if !state.settled {
                    step(state, dt, pinned);
                }
                if !state.settled {
                    return Some(canvas::Action::request_redraw());
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let Some(pos) = over else {
                    return None;
                };
                let factor = match *delta {
                    mouse::ScrollDelta::Lines { y, .. } => 1.09_f32.powf(y),
                    mouse::ScrollDelta::Pixels { y, .. } => 1.09_f32.powf(y / 48.0),
                };
                if zoom_toward(&mut state.camera, pos, factor) {
                    return Some(canvas::Action::request_redraw().and_capture());
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(pos) = over else {
                    return None;
                };
                state.press_start = Some(pos);
                state.dragged = false;
                if let Some(i) = hit {
                    let world = to_world(&state.camera, pos);
                    let b = state.bodies[i];
                    state.drag = Drag::Node {
                        index: i,
                        grab: Vector::new(world.x - b.x, world.y - b.y),
                    };
                    return Some(canvas::Action::capture());
                }
                state.drag = Drag::Pan { last: pos };
                return Some(canvas::Action::request_redraw().and_capture());
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let click = !state.dragged;
                let drag = std::mem::replace(&mut state.drag, Drag::None);
                state.press_start = None;
                state.dragged = false;
                if let Drag::Node { index, .. } = drag {
                    if click {
                        return match self.data.nodes.get(index) {
                            Some(GraphNode {
                                kind: NodeKind::Note(id),
                                ..
                            }) => Some(canvas::Action::publish(Message::Open(*id)).and_capture()),
                            Some(GraphNode {
                                kind: NodeKind::Missing(target),
                                ..
                            }) => Some(
                                canvas::Action::publish(Message::CreateLinked(target.clone()))
                                    .and_capture(),
                            ),
                            None => Some(canvas::Action::request_redraw()),
                        };
                    }
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                if matches!(drag, Drag::Pan { .. }) {
                    return Some(canvas::Action::request_redraw().and_capture());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let mut redraw = false;
                if let Some(pos) = over {
                    if let Some(start) = state.press_start
                        && (pos.x - start.x).hypot(pos.y - start.y) > DRAG_CLICK_PX {
                            state.dragged = true;
                        }
                    match state.drag {
                        Drag::Node { index, grab } => {
                            if state.dragged {
                                let world = to_world(&state.camera, pos);
                                if let Some(b) = state.bodies.get_mut(index) {
                                    b.x = world.x - grab.x;
                                    b.y = world.y - grab.y;
                                    b.vx = 0.0;
                                    b.vy = 0.0;
                                }
                                state.settled = false;
                                state.quiet = 0;
                                redraw = true;
                            }
                        }
                        Drag::Pan { last } => {
                            state.camera.pan.x += pos.x - last.x;
                            state.camera.pan.y += pos.y - last.y;
                            state.drag = Drag::Pan { last: pos };
                            redraw = true;
                        }
                        Drag::None => {}
                    }
                }
                if state.hover != hit {
                    state.hover = hit;
                    redraw = true;
                }
                if redraw {
                    let mut action = canvas::Action::request_redraw();
                    if !matches!(state.drag, Drag::None) {
                        action = action.and_capture();
                    }
                    return Some(action);
                }
            }
            Event::Mouse(mouse::Event::CursorLeft)
                if state.hover.take().is_some() => {
                    return Some(canvas::Action::request_redraw());
                }
            _ => {}
        }
        if !state.settled {
            Some(canvas::Action::request_redraw())
        } else {
            None
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        theme: &cosmic::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        if self.data.nodes.is_empty() || state.bodies.len() != self.data.nodes.len() {
            return vec![frame.into_geometry()];
        }

        let positions = screen_positions(state);
        let pal = theme.cosmic();
        let line = Color::from(pal.on_bg_color());
        let accent = Color::from(pal.accent_text_color());
        let text_color = Color::from(pal.on_bg_color());
        let n = self.data.nodes.len();
        let zoom = state.camera.zoom;
        let radius = (node_radius(n) * zoom).max(2.2);
        let selected_i = self.selected.and_then(|id| {
            self.data.nodes.iter().position(|node| match node.kind {
                NodeKind::Note(nid) => nid == id,
                NodeKind::Missing(_) => false,
            })
        });
        let focus = state.hover.or(selected_i);
        let neighbor = neighbors(n, &self.data.edges, focus);
        let degree = degrees(n, &self.data.edges);

        for &(a, b) in &self.data.edges {
            let Some((&pa, &pb)) = positions.get(a).zip(positions.get(b)) else {
                continue;
            };
            let hot = focus.is_some_and(|i| i == a || i == b);
            frame.stroke(
                &Path::line(pa, pb),
                Stroke::default()
                    .with_color(Color {
                        a: if hot { 0.55 } else { 0.16 },
                        ..line
                    })
                    .with_width(if hot { 1.6 } else { 1.0 }),
            );
        }

        for (i, point) in positions.iter().enumerate() {
            let Some(node) = self.data.nodes.get(i) else {
                continue;
            };
            let hot = focus == Some(i) || neighbor[i];
            let r = if selected_i == Some(i) {
                radius + 2.0
            } else if state.hover == Some(i) {
                radius + 1.2
            } else {
                radius
            };
            match node.kind {
                NodeKind::Note(_) => {
                    let fill = Color {
                        a: if hot || selected_i == Some(i) {
                            0.95
                        } else {
                            0.82
                        },
                        ..accent
                    };
                    frame.fill(&Path::circle(*point, r), Fill::from(fill));
                    if selected_i == Some(i) {
                        frame.stroke(
                            &Path::circle(*point, r + 3.0),
                            Stroke::default().with_color(accent).with_width(1.4),
                        );
                    }
                }
                NodeKind::Missing(_) => {
                    frame.stroke(
                        &Path::circle(*point, r),
                        Stroke::default()
                            .with_color(Color { a: 0.5, ..line })
                            .with_width(1.2),
                    );
                }
            }
        }

        let labeled = labels_to_show(n, state.hover, selected_i, &neighbor, &degree);
        let mut placed: Vec<Point> = Vec::new();
        let size = bounds.size();
        for i in labeled {
            let Some(point) = positions.get(i) else {
                continue;
            };
            let Some(node) = self.data.nodes.get(i) else {
                continue;
            };
            let right = point.x < size.width * 0.62;
            let label_pos = Point::new(
                point.x + if right { radius + 5.0 } else { -(radius + 5.0) },
                point.y,
            );
            if !must_label(i, state.hover, selected_i)
                && placed.iter().any(|p| {
                    (p.x - label_pos.x).abs() < 72.0 && (p.y - label_pos.y).abs() < 13.0
                })
            {
                continue;
            }
            placed.push(label_pos);
            let dim = match node.kind {
                NodeKind::Note(_) => 0.88,
                NodeKind::Missing(_) => 0.55,
            };
            frame.fill_text(Text {
                content: truncate(&node.label, 28),
                position: label_pos,
                color: Color {
                    a: dim,
                    ..text_color
                },
                size: Pixels(if state.hover == Some(i) { 12.0 } else { 11.0 }),
                align_x: if right {
                    alignment::Horizontal::Left.into()
                } else {
                    alignment::Horizontal::Right.into()
                },
                align_y: alignment::Vertical::Center,
                ..Text::default()
            });
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match state.drag {
            Drag::Node { .. } => return mouse::Interaction::Grabbing,
            Drag::Pan { .. } => return mouse::Interaction::Grabbing,
            Drag::None => {}
        }
        let Some(pos) = cursor.position_in(bounds) else {
            return mouse::Interaction::default();
        };
        if state.bodies.len() != self.data.nodes.len() {
            return mouse::Interaction::Grab;
        }
        if hit_index(state, pos, self.data.nodes.len()).is_some() {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::Grab
        }
    }
}

fn sync(state: &mut PlotState, data: &GraphData, size: Size) {
    if !state.camera_ready && size.width > 8.0 && size.height > 8.0 {
        state.camera.pan = Vector::new(size.width * 0.5, size.height * 0.5);
        state.camera_ready = true;
    }
    let fp = topology_fingerprint(data);
    if fp == state.topology && state.bodies.len() == data.nodes.len() {
        return;
    }
    let mut old: HashMap<NodeKey, Body> = HashMap::new();
    for (key, body) in state.keys.iter().zip(state.bodies.iter()) {
        old.insert(key.clone(), *body);
    }
    let had = !old.is_empty();
    state.keys = data.nodes.iter().map(node_key).collect();
    state.edges = data.edges.clone();
    state.topology = fp;
    let seed = seed_layout(data.nodes.len(), &data.edges);
    let mut bodies = Vec::with_capacity(data.nodes.len());
    let mut fresh = Vec::new();
    for (i, key) in state.keys.iter().enumerate() {
        if let Some(body) = old.get(key) {
            bodies.push(*body);
        } else {
            bodies.push(Body {
                x: seed.get(i).map(|p| p.x).unwrap_or(0.0),
                y: seed.get(i).map(|p| p.y).unwrap_or(0.0),
                vx: 0.0,
                vy: 0.0,
            });
            fresh.push(i);
        }
    }
    if had {
        let cx = old.values().map(|b| b.x).sum::<f32>() / old.len() as f32;
        let cy = old.values().map(|b| b.y).sum::<f32>() / old.len() as f32;
        for (k, &i) in fresh.iter().enumerate() {
            if let Some(nb) = neighbor_of(i, &state.edges, data.nodes.len())
                .find(|&j| !fresh.contains(&j))
            {
                let gold = PI * (3.0 - 5.0_f32.sqrt());
                let t = k as f32 * gold;
                bodies[i].x = bodies[nb].x + 28.0 * t.cos();
                bodies[i].y = bodies[nb].y + 28.0 * t.sin();
            } else {
                let gold = PI * (3.0 - 5.0_f32.sqrt());
                let t = k as f32 * gold;
                bodies[i].x = cx + 36.0 * t.cos();
                bodies[i].y = cy + 36.0 * t.sin();
            }
            bodies[i].vx = 0.0;
            bodies[i].vy = 0.0;
        }
    }
    state.bodies = bodies;
    state.settled = data.nodes.len() <= 1;
    state.quiet = 0;
    state.last = Instant::now();
}

fn step(state: &mut PlotState, dt: f32, pinned: Option<usize>) {
    let n = state.bodies.len();
    if n == 0 {
        state.settled = true;
        return;
    }
    let dt = dt.clamp(1.0 / 240.0, 0.05);
    let max_v = tick(&mut state.bodies, &state.edges, dt, pinned);
    if pinned.is_some() {
        state.quiet = 0;
        state.settled = false;
        return;
    }
    if max_v < SETTLE_SPEED {
        state.quiet = state.quiet.saturating_add(1);
        if state.quiet >= SETTLE_FRAMES {
            for b in &mut state.bodies {
                b.vx = 0.0;
                b.vy = 0.0;
            }
            state.settled = true;
        }
    } else {
        state.quiet = 0;
    }
}

fn tick(bodies: &mut [Body], edges: &[(usize, usize)], dt: f32, pinned: Option<usize>) -> f32 {
    let steps = (dt * 60.0).round().clamp(1.0, 3.0) as u32;
    let mut max_v = 0.0f32;
    for _ in 0..steps {
        max_v = tick_once(bodies, edges, pinned);
    }
    max_v
}

fn tick_once(bodies: &mut [Body], edges: &[(usize, usize)], pinned: Option<usize>) -> f32 {
    let n = bodies.len();
    let mut ax = vec![0.0f32; n];
    let mut ay = vec![0.0f32; n];
    apply_repel(bodies, &mut ax, &mut ay);
    for &(a, b) in edges {
        if a >= n || b >= n || a == b {
            continue;
        }
        let dx = bodies[b].x - bodies[a].x;
        let dy = bodies[b].y - bodies[a].y;
        let d = (dx * dx + dy * dy).sqrt().max(0.01);
        let mag = LINK_K * (d - LINK_DIST);
        let fx = dx / d * mag;
        let fy = dy / d * mag;
        ax[a] += fx;
        ay[a] += fy;
        ax[b] -= fx;
        ay[b] -= fy;
    }
    for i in 0..n {
        ax[i] -= bodies[i].x * GRAVITY;
        ay[i] -= bodies[i].y * GRAVITY;
    }
    let mut max_v = 0.0f32;
    for i in 0..n {
        if pinned == Some(i) {
            bodies[i].vx = 0.0;
            bodies[i].vy = 0.0;
            continue;
        }
        let mut vx = (bodies[i].vx + ax[i]) * DAMPING;
        let mut vy = (bodies[i].vy + ay[i]) * DAMPING;
        let speed = vx.hypot(vy);
        if speed > MAX_SPEED {
            let s = MAX_SPEED / speed;
            vx *= s;
            vy *= s;
        }
        bodies[i].vx = vx;
        bodies[i].vy = vy;
        bodies[i].x += vx;
        bodies[i].y += vy;
        max_v = max_v.max(vx.hypot(vy));
    }
    max_v
}

fn apply_repel(bodies: &[Body], ax: &mut [f32], ay: &mut [f32]) {
    let n = bodies.len();
    if n <= 140 {
        for i in 0..n {
            for j in (i + 1)..n {
                pair_repel(bodies, ax, ay, i, j);
            }
        }
        return;
    }
    let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (i, b) in bodies.iter().enumerate() {
        let cx = (b.x / REPEL_CELL).floor() as i32;
        let cy = (b.y / REPEL_CELL).floor() as i32;
        cells.entry((cx, cy)).or_default().push(i);
    }
    for (i, b) in bodies.iter().enumerate() {
        let cx = (b.x / REPEL_CELL).floor() as i32;
        let cy = (b.y / REPEL_CELL).floor() as i32;
        for ox in -1..=1 {
            for oy in -1..=1 {
                let Some(bucket) = cells.get(&(cx + ox, cy + oy)) else {
                    continue;
                };
                for &j in bucket {
                    if j > i {
                        pair_repel(bodies, ax, ay, i, j);
                    }
                }
            }
        }
    }
}

fn pair_repel(bodies: &[Body], ax: &mut [f32], ay: &mut [f32], i: usize, j: usize) {
    let mut dx = bodies[j].x - bodies[i].x;
    let mut dy = bodies[j].y - bodies[i].y;
    let mut d2 = dx * dx + dy * dy;
    if d2 < 0.01 {
        dx = 0.7;
        dy = 0.3;
        d2 = dx * dx + dy * dy;
    }
    let mag = CHARGE / d2;
    let fx = dx * mag;
    let fy = dy * mag;
    ax[i] -= fx;
    ay[i] -= fy;
    ax[j] += fx;
    ay[j] += fy;
}

fn zoom_toward(camera: &mut Camera, cursor: Point, factor: f32) -> bool {
    if (factor - 1.0).abs() < 1e-6 {
        return false;
    }
    let old = camera.zoom;
    let new = (old * factor).clamp(ZOOM_MIN, ZOOM_MAX);
    if (new - old).abs() < 1e-6 {
        return false;
    }
    let world = to_world(camera, cursor);
    camera.zoom = new;
    camera.pan.x = cursor.x - world.x * new;
    camera.pan.y = cursor.y - world.y * new;
    true
}

fn to_screen(camera: &Camera, x: f32, y: f32) -> Point {
    Point::new(x * camera.zoom + camera.pan.x, y * camera.zoom + camera.pan.y)
}

fn to_world(camera: &Camera, p: Point) -> Point {
    Point::new(
        (p.x - camera.pan.x) / camera.zoom,
        (p.y - camera.pan.y) / camera.zoom,
    )
}

fn screen_positions(state: &PlotState) -> Vec<Point> {
    state
        .bodies
        .iter()
        .map(|b| to_screen(&state.camera, b.x, b.y))
        .collect()
}

fn hit_index(state: &PlotState, pos: Point, n: usize) -> Option<usize> {
    if state.bodies.len() != n {
        return None;
    }
    let r = node_radius(n) * state.camera.zoom + 8.0;
    let r2 = r * r;
    state
        .bodies
        .iter()
        .map(|b| to_screen(&state.camera, b.x, b.y))
        .enumerate()
        .filter_map(|(i, p)| {
            let dx = p.x - pos.x;
            let dy = p.y - pos.y;
            let d2 = dx * dx + dy * dy;
            (d2 <= r2).then_some((d2, i))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, i)| i)
}

fn node_key(node: &GraphNode) -> NodeKey {
    match &node.kind {
        NodeKind::Note(id) => NodeKey::Note(*id),
        NodeKind::Missing(t) => NodeKey::Missing(t.clone()),
    }
}

fn topology_fingerprint(data: &GraphData) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.nodes.len().hash(&mut hasher);
    for node in &data.nodes {
        match &node.kind {
            NodeKind::Note(id) => {
                0u8.hash(&mut hasher);
                id.hash(&mut hasher);
            }
            NodeKind::Missing(target) => {
                1u8.hash(&mut hasher);
                target.hash(&mut hasher);
            }
        }
        node.label.hash(&mut hasher);
    }
    data.edges.hash(&mut hasher);
    hasher.finish()
}

fn neighbor_of(
    i: usize,
    edges: &[(usize, usize)],
    n: usize,
) -> impl Iterator<Item = usize> + '_ {
    edges.iter().filter_map(move |&(a, b)| {
        if a == i && b < n {
            Some(b)
        } else if b == i && a < n {
            Some(a)
        } else {
            None
        }
    })
}

fn seed_layout(n: usize, edges: &[(usize, usize)]) -> Vec<Point> {
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![Point::ORIGIN];
    }
    let w: f32 = 720.0;
    let h: f32 = 560.0;
    let pad: f32 = 48.0;
    let inner_w = (w - 2.0 * pad).max(48.0);
    let inner_h = (h - 2.0 * pad).max(48.0);
    let comps = components(n, edges);
    let max_r = (inner_w.min(inner_h) * 0.48).max(12.0);
    let centroids = sunflower(comps.len(), inner_w, inner_h);
    let mut pos = vec![Point::ORIGIN; n];
    for (ci, comp) in comps.iter().enumerate() {
        let c = centroids[ci];
        if comp.len() == 1 {
            pos[comp[0]] = c;
            continue;
        }
        let share = (comp.len() as f32 / n as f32).sqrt().max(0.12);
        let span = (max_r * share * 1.7).max(28.0);
        let local = sunflower(comp.len(), span, span);
        let ox = c.x - span / 2.0;
        let oy = c.y - span / 2.0;
        for (k, &i) in comp.iter().enumerate() {
            pos[i] = Point::new(ox + local[k].x, oy + local[k].y);
        }
    }
    let min_d = node_radius(n) * 2.6 + 6.0;
    separate(&mut pos, min_d);
    fit(&mut pos, w, h, pad);
    let (cx, cy) = centroid(&pos);
    for p in &mut pos {
        p.x -= cx;
        p.y -= cy;
    }
    pos
}

fn centroid(pos: &[Point]) -> (f32, f32) {
    if pos.is_empty() {
        return (0.0, 0.0);
    }
    let n = pos.len() as f32;
    (
        pos.iter().map(|p| p.x).sum::<f32>() / n,
        pos.iter().map(|p| p.y).sum::<f32>() / n,
    )
}

fn components(n: usize, edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for &(a, b) in edges {
        if a >= n || b >= n {
            continue;
        }
        let pa = find(&mut parent, a);
        let pb = find(&mut parent, b);
        if pa != pb {
            parent[pa] = pb;
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        groups.entry(find(&mut parent, i)).or_default().push(i);
    }
    let mut comps: Vec<Vec<usize>> = groups.into_values().collect();
    comps.sort_by_key(|c| std::cmp::Reverse(c.len()));
    comps
}

fn sunflower(n: usize, w: f32, h: f32) -> Vec<Point> {
    let cx = w / 2.0;
    let cy = h / 2.0;
    let max_r = (w.min(h) * 0.48).max(12.0);
    let golden = PI * (3.0 - 5.0_f32.sqrt());
    (0..n)
        .map(|i| {
            let r = max_r * ((i as f32 + 0.5) / n as f32).sqrt();
            let t = i as f32 * golden;
            Point::new(cx + r * t.cos(), cy + r * t.sin())
        })
        .collect()
}

fn separate(pos: &mut [Point], min_d: f32) {
    let min2 = min_d * min_d;
    for _ in 0..8 {
        for i in 0..pos.len() {
            for j in (i + 1)..pos.len() {
                let dx = pos[j].x - pos[i].x;
                let dy = pos[j].y - pos[i].y;
                let d2 = dx * dx + dy * dy;
                if d2 == 0.0 {
                    pos[j].x += min_d * 0.5;
                    continue;
                }
                if d2 < min2 {
                    let d = d2.sqrt();
                    let push = (min_d - d) * 0.5;
                    let ux = dx / d;
                    let uy = dy / d;
                    pos[i].x -= ux * push;
                    pos[i].y -= uy * push;
                    pos[j].x += ux * push;
                    pos[j].y += uy * push;
                }
            }
        }
    }
}

fn fit(pos: &mut [Point], w: f32, h: f32, pad: f32) {
    if pos.is_empty() {
        return;
    }
    let mut minx = f32::MAX;
    let mut maxx = f32::MIN;
    let mut miny = f32::MAX;
    let mut maxy = f32::MIN;
    for p in pos.iter() {
        minx = minx.min(p.x);
        maxx = maxx.max(p.x);
        miny = miny.min(p.y);
        maxy = maxy.max(p.y);
    }
    let dx = (maxx - minx).max(1.0);
    let dy = (maxy - miny).max(1.0);
    let scale = ((w - 2.0 * pad).max(1.0) / dx).min((h - 2.0 * pad).max(1.0) / dy);
    let ox = (w - dx * scale) / 2.0;
    let oy = (h - dy * scale) / 2.0;
    for p in pos.iter_mut() {
        p.x = ox + (p.x - minx) * scale;
        p.y = oy + (p.y - miny) * scale;
    }
}

fn node_radius(n: usize) -> f32 {
    match n {
        0..=12 => 7.0,
        13..=28 => 5.5,
        29..=55 => 4.4,
        _ => 3.4,
    }
}

fn degrees(n: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut d = vec![0usize; n];
    for &(a, b) in edges {
        if a < n {
            d[a] += 1;
        }
        if b < n {
            d[b] += 1;
        }
    }
    d
}

fn neighbors(n: usize, edges: &[(usize, usize)], focus: Option<usize>) -> Vec<bool> {
    let mut out = vec![false; n];
    let Some(i) = focus else {
        return out;
    };
    for &(a, b) in edges {
        if a == i && b < n {
            out[b] = true;
        } else if b == i && a < n {
            out[a] = true;
        }
    }
    out
}

fn labels_to_show(
    n: usize,
    hover: Option<usize>,
    selected: Option<usize>,
    neighbor: &[bool],
    degree: &[usize],
) -> Vec<usize> {
    if n <= 18 {
        return (0..n).collect();
    }
    let mut show = vec![false; n];
    if let Some(i) = hover {
        show[i] = true;
    }
    if let Some(i) = selected {
        show[i] = true;
    }
    for (i, nbor) in neighbor.iter().enumerate() {
        if *nbor {
            show[i] = true;
        }
    }
    let mut hubs: Vec<(usize, usize)> = degree
        .iter()
        .enumerate()
        .filter(|(i, d)| **d >= 2 && !show[*i])
        .map(|(i, d)| (*d, i))
        .collect();
    hubs.sort_by(|a, b| b.0.cmp(&a.0));
    let labeled = show.iter().filter(|s| **s).count();
    for &(_, i) in hubs.iter().take(12usize.saturating_sub(labeled)) {
        show[i] = true;
    }
    show.iter()
        .enumerate()
        .filter(|(_, s)| **s)
        .map(|(i, _)| i)
        .collect()
}

fn must_label(i: usize, hover: Option<usize>, selected: Option<usize>) -> bool {
    hover == Some(i) || selected == Some(i)
}

fn truncate(s: &str, max_chars: usize) -> String {
    let n = s.chars().count();
    if n <= max_chars {
        s.to_string()
    } else {
        format!(
            "{}…",
            s.chars().take(max_chars.saturating_sub(1)).collect::<String>()
        )
    }
}

fn ordered(a: usize, b: usize) -> (usize, usize) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

fn normalize(s: &str) -> String {
    markdown::normalize_target(s)
}

fn basename(s: &str) -> &str {
    markdown::basename_of(s)
}

fn resolve<'a>(index: &'a HashMap<String, usize>, key: &str) -> Option<&'a usize> {
    index.get(key).or_else(|| {
        let base = basename(key);
        if base != key {
            index.get(base)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: u64, title: &str, body: &str) -> Note {
        Note {
            id,
            title: title.to_string(),
            body_md: body.to_string(),
            revision: 1,
            folder_id: 0,
        }
    }

    fn dist(a: &Body, b: &Body) -> f32 {
        (a.x - b.x).hypot(a.y - b.y)
    }

    #[test]
    fn path_wikilink_matches_title_basename() {
        let mut notes = HashMap::new();
        notes.insert(1, note(1, "Overview", ""));
        notes.insert(2, note(2, "Journal", "see [[Life Context/Overview]]"));
        let g = build(&notes);
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1);
        assert!(g.nodes.iter().all(|n| matches!(n.kind, NodeKind::Note(_))));
    }

    #[test]
    fn missing_target_becomes_a_node() {
        let mut notes = HashMap::new();
        notes.insert(1, note(1, "Journal", "see [[No Such Page]]"));
        let g = build(&notes);
        assert_eq!(g.nodes.len(), 2);
        assert!(g
            .nodes
            .iter()
            .any(|n| matches!(&n.kind, NodeKind::Missing(t) if t == "No Such Page")));
        assert_eq!(g.edges.len(), 1);
    }

    #[test]
    fn seed_is_not_a_thin_ring() {
        let n = 90;
        let pos = seed_layout(n, &[]);
        assert_eq!(pos.len(), n);
        let rs: Vec<f32> = pos.iter().map(|p| p.x.hypot(p.y)).collect();
        let mean = rs.iter().sum::<f32>() / n as f32;
        let var = rs.iter().map(|r| (r - mean).powi(2)).sum::<f32>() / n as f32;
        assert!(
            var > 800.0,
            "radii should fan out, not sit on one circle (var={var})"
        );
        let mut min_d = f32::MAX;
        for i in 0..n {
            for j in (i + 1)..n {
                min_d = min_d.min((pos[i].x - pos[j].x).hypot(pos[i].y - pos[j].y));
            }
        }
        assert!(min_d > 8.0, "nodes should not pile up (min_d={min_d})");
    }

    #[test]
    fn nodes_repel() {
        let mut bodies = [
            Body {
                x: 0.0,
                y: 0.0,
                vx: 0.0,
                vy: 0.0,
            },
            Body {
                x: 8.0,
                y: 0.0,
                vx: 0.0,
                vy: 0.0,
            },
        ];
        let before = dist(&bodies[0], &bodies[1]);
        for _ in 0..40 {
            tick(&mut bodies, &[], 1.0 / 60.0, None);
        }
        let after = dist(&bodies[0], &bodies[1]);
        assert!(after > before + 4.0, "repel: {before} -> {after}");
    }

    #[test]
    fn links_pull() {
        let mut bodies = [
            Body {
                x: 0.0,
                y: 0.0,
                vx: 0.0,
                vy: 0.0,
            },
            Body {
                x: 320.0,
                y: 0.0,
                vx: 0.0,
                vy: 0.0,
            },
        ];
        let before = dist(&bodies[0], &bodies[1]);
        for _ in 0..50 {
            tick(&mut bodies, &[(0, 1)], 1.0 / 60.0, None);
        }
        let after = dist(&bodies[0], &bodies[1]);
        assert!(after < before - 20.0, "spring: {before} -> {after}");
    }

    #[test]
    fn simulation_settles() {
        let seed = seed_layout(12, &[(0, 1), (1, 2), (2, 3), (3, 0), (4, 5)]);
        let mut bodies: Vec<Body> = seed
            .iter()
            .map(|p| Body {
                x: p.x,
                y: p.y,
                vx: 0.0,
                vy: 0.0,
            })
            .collect();
        let mut max_v = f32::MAX;
        for _ in 0..180 {
            max_v = tick(
                &mut bodies,
                &[(0, 1), (1, 2), (2, 3), (3, 0), (4, 5)],
                1.0 / 60.0,
                None,
            );
        }
        assert!(
            max_v < SETTLE_SPEED,
            "should come to rest (max_v={max_v})"
        );
    }

    #[test]
    fn zoom_keeps_the_cursor_world_point() {
        let mut cam = Camera {
            pan: Vector::new(120.0, 80.0),
            zoom: 1.0,
        };
        let cursor = Point::new(240.0, 160.0);
        let before = to_world(&cam, cursor);
        assert!(zoom_toward(&mut cam, cursor, 1.25));
        let after = to_world(&cam, cursor);
        assert!((before.x - after.x).abs() < 0.02);
        assert!((before.y - after.y).abs() < 0.02);
        assert!((cam.zoom - 1.25).abs() < 1e-5);
    }
}
