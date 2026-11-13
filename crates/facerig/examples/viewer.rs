//! Procedural face rig with live sliders for identity, FACS action units and bones.
//!
//! Drag to orbit, scroll to zoom, Esc to quit.
//! `cargo run --release --example viewer -- [preset] [--yaw RAD] [--landmarks] [--shot out.png]` starts on a preset (`smile`,
//! `surprise`, `anger`, `blink`, `talk`), orbits the camera by `--yaw`, shows the landmarks and
//! their triangulation with `--landmarks` and, with `--shot`, saves a screenshot after a few
//! frames and exits. `--look X,Y` pins the cursor the eyes follow, for screenshots. `--low`
//! starts in low-poly mode; `--ui` keeps the panel in screenshots.

use game_toolkit::prelude::*;
use game_toolkit_facerig::{Detail, FaceModel, FaceParams, FaceRig, Group, PARAMS, Param};

/// How long after the last interaction (click, drag, scroll, key, UI use) the head starts to
/// follow the cursor again, in seconds.
const HEAD_DELAY: f32 = 1.5;

/// How fast the shown face morphs toward new slider values (per second; about 95% of the way
/// in 3 / `TWEEN_RATE` seconds).
const TWEEN_RATE: f32 = 8.0;

/// Width of the slider panel, in points.
const PANEL_WIDTH: f32 = 300.0;

type Preset = (&'static str, &'static [(Param, f32)]);

const PRESETS: &[Preset] = &[
    ("neutral", &[]),
    (
        "smile",
        &[
            (Param::LipCornerPuller, 0.9),
            (Param::CheekRaiser, 0.6),
            (Param::LipsPart, 0.4),
        ],
    ),
    (
        "surprise",
        &[
            (Param::InnerBrowRaiser, 1.0),
            (Param::OuterBrowRaiser, 1.0),
            (Param::UpperLidRaiser, 0.8),
            (Param::LipsPart, 0.6),
            (Param::JawOpen, 0.15),
        ],
    ),
    (
        "anger",
        &[
            // FACS anger: AU4 + AU5 + AU7 + AU23.
            (Param::BrowLowerer, 1.0),
            (Param::UpperLidRaiser, 0.4),
            (Param::LidTightener, 0.6),
            (Param::LipTightener, 0.9),
        ],
    ),
    ("blink", &[(Param::EyesClosed, 1.0)]),
    (
        "talk",
        &[
            (Param::LipsPart, 0.7),
            (Param::LipPucker, 0.3),
            (Param::JawOpen, 0.15),
        ],
    ),
];

/// `base` with its expression and jaw replaced by preset `name`; identity, head and gaze stay.
fn apply_preset(base: &FaceParams, name: &str) -> Option<FaceParams> {
    let (_, values) = PRESETS.iter().find(|(n, _)| *n == name)?;
    let mut p = base.clone();
    p.reset_group(Group::Expression);
    for jaw in [Param::JawOpen, Param::JawSide, Param::JawForward] {
        p.set(jaw, 0.0);
    }
    for &(param, v) in *values {
        p.set(param, v);
    }
    Some(p)
}

struct Orbit {
    yaw: f32,
    pitch: f32,
    distance: f32,
}

struct App {
    ui: Ui,
    rig: FaceRig,
    model: FaceModel,
    /// The rig and model of the other detail level, kept once built so toggling is instant.
    other: Option<(FaceRig, FaceModel)>,
    /// Show the faceted low-poly mesh.
    low_poly: bool,
    /// What the sliders say: the target the face morphs toward.
    params: FaceParams,
    /// What the rig shows, eased toward `params`.
    shown: FaceParams,
    orbit: Orbit,
    show_landmarks: bool,
    show_ids: bool,
    show_mesh: bool,
    /// The eyes always track the cursor (except in `--shot` mode without `--look`).
    eyes_follow: bool,
    /// The head also turns toward the cursor, paused for [`HEAD_DELAY`] after any interaction.
    head_follow: bool,
    /// Seconds since the last interaction.
    idle: f32,
    /// `--look X,Y`: a fixed cursor position in pixels, for screenshots.
    look: Option<[f32; 2]>,
    /// `--shot`: output path and frames rendered so far.
    shot: Option<(String, u32)>,
    /// `--ui`: keep the panel in screenshots.
    shot_ui: bool,
}

impl Game for App {
    fn init(ctx: &mut Context) -> Result<Self> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let flag = |name: &str| {
            args.iter()
                .position(|a| a == name)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        let params = args
            .first()
            .and_then(|a| apply_preset(&FaceParams::default(), a))
            .unwrap_or_default();
        let yaw = flag("--yaw").and_then(|y| y.parse().ok()).unwrap_or(0.0);
        let low_poly = args.iter().any(|a| a == "--low");
        let mut rig = FaceRig::with_detail(if low_poly {
            Detail::Low
        } else {
            Detail::Standard
        });
        rig.set_params(&params);
        let model = FaceModel::new(&mut ctx.gfx, &rig);
        Ok(Self {
            ui: Ui::new(&ctx.gfx)?,
            rig,
            model,
            other: None,
            low_poly,
            shown: params.clone(),
            params,
            orbit: Orbit {
                yaw,
                pitch: 0.05,
                distance: 4.2,
            },
            show_landmarks: args.iter().any(|a| a == "--landmarks"),
            show_ids: false,
            show_mesh: args.iter().any(|a| a == "--landmarks"),
            eyes_follow: flag("--shot").is_none() || flag("--look").is_some(),
            head_follow: true,
            idle: 0.0,
            look: flag("--look").and_then(|s| {
                let (x, y) = s.split_once(',')?;
                Some([x.parse().ok()?, y.parse().ok()?])
            }),
            shot: flag("--shot").map(|p| (p, 0)),
            shot_ui: args.iter().any(|a| a == "--ui"),
        })
    }

    fn raw_window_event(&mut self, _ctx: &mut Context, event: &winit::event::WindowEvent) -> bool {
        self.ui.on_window_event(event)
    }

    fn update(&mut self, ctx: &mut Context, dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
        if let Some((path, frames)) = &mut self.shot {
            *frames += 1;
            if *frames == 3 {
                ctx.gfx.request_screenshot(path.clone());
            } else if *frames > 5 {
                ctx.quit();
            }
        }

        if !self.ui.ctx.egui_wants_pointer_input() {
            if ctx.input.mouse_held(MouseButton::Left) {
                let (dx, dy) = ctx.input.mouse_delta();
                self.orbit.yaw -= dx * 0.008;
                self.orbit.pitch = (self.orbit.pitch + dy * 0.008).clamp(-1.4, 1.4);
            }
            let (_, wheel) = ctx.input.scroll();
            self.orbit.distance = (self.orbit.distance * (1.0 - wheel * 0.1)).clamp(1.5, 12.0);
        }
        // The camera must be set in `update`: it is uploaded before `render` runs.
        let o = &self.orbit;
        let target = [0.0, -0.1, 0.0];
        let cam = &mut ctx.gfx.camera3d;
        cam.target = target;
        cam.eye = [
            target[0] + o.distance * o.pitch.cos() * o.yaw.sin(),
            target[1] + o.distance * o.pitch.sin(),
            target[2] + o.distance * o.pitch.cos() * o.yaw.cos(),
        ];
        cam.fov_y = 0.6;

        if self.eyes_follow {
            let (mx, my) = ctx.input.mouse_pos();
            let cursor = self.look.unwrap_or([mx, my]);
            let input = &ctx.input;
            let interacting = input.any_pressed()
                || [MouseButton::Left, MouseButton::Right, MouseButton::Middle]
                    .into_iter()
                    .any(|b| input.mouse_held(b))
                || input.scroll() != (0.0, 0.0)
                || self.ui.ctx.egui_is_using_pointer()
                || self.ui.ctx.egui_wants_keyboard_input();
            self.idle = if interacting { 0.0 } else { self.idle + dt };
            let (w, h) = ctx.gfx.size();
            let target = cursor_target(&ctx.gfx.camera3d, cursor, [w as f32, h as f32]);
            // The head holds still while you interact (orbiting, dragging sliders) and for a
            // moment after, then follows again. A pinned look (screenshots) skips the wait and
            // snaps instead of easing in over a few frames.
            let pinned = self.look.is_some();
            let move_head = self.head_follow && (pinned || self.idle > HEAD_DELAY);
            self.track(target, if pinned { 2.0 } else { dt }, move_head);
        }

        if self.low_poly != (self.rig.topology().detail == Detail::Low) {
            let (rig, model) = self.other.take().unwrap_or_else(|| {
                let detail = if self.low_poly {
                    Detail::Low
                } else {
                    Detail::Standard
                };
                let rig = FaceRig::with_detail(detail);
                let model = FaceModel::new(&mut ctx.gfx, &rig);
                (rig, model)
            });
            let old_rig = std::mem::replace(&mut self.rig, rig);
            let old_model = std::mem::replace(&mut self.model, model);
            self.other = Some((old_rig, old_model));
            // Bring the swapped-in rig to the current pose (it may be stale or fresh).
            self.rig.set_params(&self.shown);
            self.model.sync(&mut ctx.gfx, &self.rig);
        }
        self.tween(dt);
        if &self.shown != self.rig.params() {
            self.rig.set_params(&self.shown);
            self.model.sync(&mut ctx.gfx, &self.rig);
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let (w, h) = ctx.gfx.size();
        let view_proj = ctx.gfx.camera3d.view_proj();
        {
            let mut p = frame.painter(&mut ctx.gfx);
            p.clear([0.13, 0.14, 0.17, 1.0]);
            self.model.draw(&mut p, &self.rig, transform::identity());
            if self.show_landmarks {
                let screen = self
                    .rig
                    .landmarks()
                    .map(|lm| project(&view_proj, lm, w as f32, h as f32));
                if self.show_mesh {
                    let mut edges: Vec<(usize, usize)> = self
                        .rig
                        .landmark_triangles()
                        .iter()
                        .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
                        .map(|(a, b)| (a.min(b), a.max(b)))
                        .collect();
                    edges.sort_unstable();
                    edges.dedup();
                    for (a, b) in edges {
                        if let (Some(pa), Some(pb)) = (screen[a], screen[b]) {
                            p.line(pa, pb, 1.0, [0.2, 1.0, 0.4, 0.5]);
                        }
                    }
                }
                for (i, s) in screen.iter().enumerate() {
                    let Some(s) = *s else { continue };
                    p.circle(s, 3.0, [0.2, 1.0, 0.4, 1.0]);
                    if self.show_ids {
                        p.text([s[0] + 4.0, s[1] - 6.0], &i.to_string(), 11.0, [1.0; 4]);
                    }
                }
            }
        }
        if self.shot.is_some() && !self.shot_ui {
            return;
        }

        let params = &mut self.params;
        let show_landmarks = &mut self.show_landmarks;
        let show_ids = &mut self.show_ids;
        let show_mesh = &mut self.show_mesh;
        let head_follow = &mut self.head_follow;
        let low_poly = &mut self.low_poly;
        let rng = &mut ctx.rng;
        self.ui.run(&mut ctx.gfx, frame, |ectx| {
            egui::Window::new("face rig")
                .default_width(PANEL_WIDTH)
                .show(ectx, |ui| {
                    // A fixed width: rows wrap instead of widening the panel over the face.
                    ui.set_max_width(PANEL_WIDTH);
                    ui.horizontal_wrapped(|ui| {
                        for (name, _) in PRESETS {
                            if ui.button(*name).clicked()
                                && let Some(p) = apply_preset(params, name)
                            {
                                *params = p;
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("reset all").clicked() {
                            *params = FaceParams::default();
                        }
                        if ui.button("randomize all").clicked() {
                            for group in [Group::Identity, Group::Expression, Group::Pose] {
                                params.randomize_group(group, || rng.next_f32());
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(low_poly, "low-poly");
                        ui.checkbox(head_follow, "head follows cursor")
                            .on_hover_text(
                                "pauses briefly whenever you interact; drives head yaw/pitch",
                            );
                        ui.checkbox(show_landmarks, "landmarks");
                        ui.add_enabled(*show_landmarks, egui::Checkbox::new(show_ids, "ids"));
                        ui.add_enabled(
                            *show_landmarks,
                            egui::Checkbox::new(show_mesh, "triangulation"),
                        );
                    });
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (group, title) in [
                            (Group::Identity, "Identity"),
                            (Group::Expression, "Expression (FACS)"),
                            (Group::Pose, "Pose (bones)"),
                        ] {
                            egui::CollapsingHeader::new(title)
                                .default_open(group != Group::Identity)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        if ui.small_button("reset").clicked() {
                                            params.reset_group(group);
                                        }
                                        if ui.small_button("randomize").clicked() {
                                            params.randomize_group(group, || rng.next_f32());
                                        }
                                    });
                                    let values = params.as_mut_slice();
                                    for s in PARAMS.iter().filter(|s| s.group == group) {
                                        ui.add(
                                            egui::Slider::new(
                                                &mut values[s.param as usize],
                                                s.min..=s.max,
                                            )
                                            .text(s.label),
                                        );
                                    }
                                });
                        }
                    });
                });
        });
    }
}

impl App {
    /// Morph the shown face toward the slider values instead of jumping. Gaze and head turn
    /// are already eased by [`App::track`] while the cursor drives them, so they pass through.
    fn tween(&mut self, dt: f32) {
        let tracked = |p: Param| match p {
            Param::EyeYaw | Param::EyePitch => self.eyes_follow,
            Param::HeadYaw | Param::HeadPitch => self.eyes_follow && self.head_follow,
            _ => false,
        };
        // Screenshots show the final pose.
        let k = if self.shot.is_some() {
            1.0
        } else {
            1.0 - (-TWEEN_RATE * dt).exp()
        };
        let target = self.params.as_slice();
        for (s, (v, &t)) in PARAMS
            .iter()
            .zip(self.shown.as_mut_slice().iter_mut().zip(target))
        {
            let d = t - *v;
            // Snap the last bit so the rig stops re-evaluating once settled.
            *v = if tracked(s.param) || d.abs() < 1e-4 * (s.max - s.min) {
                t
            } else {
                *v + d * k
            };
        }
    }

    /// Turn the eyes toward `target` (world space). With `move_head`, the head takes a small
    /// share of the turn, and more once the eyes reach their limit. Both ease in, the head more
    /// slowly.
    fn track(&mut self, target: [f32; 3], dt: f32, move_head: bool) {
        // The whole turn, measured as gaze angles for an unturned head.
        let mut unturned = self.params.clone();
        unturned.set(Param::HeadYaw, 0.0);
        unturned.set(Param::HeadPitch, 0.0);
        let [yaw, pitch] = self.rig.gaze_toward(&unturned, target);
        // The eyes take most of a small turn; past what the lid opening shows, the head turns
        // to keep the iris in view.
        const EYE_SHARE: f32 = 0.8;
        const EYE_MAX: [f32; 2] = [0.3, 0.2];
        let head_goal = |total: f32, max: f32| total - (EYE_SHARE * total).clamp(-max, max);
        let ease = |cur: f32, goal: f32, rate: f32| cur + (goal - cur) * (1.0 - (-rate * dt).exp());
        if move_head {
            let p = &mut self.params;
            let head_yaw = ease(p.get(Param::HeadYaw), head_goal(yaw, EYE_MAX[0]), 3.0);
            let head_pitch = ease(p.get(Param::HeadPitch), head_goal(pitch, EYE_MAX[1]), 3.0);
            p.set(Param::HeadYaw, head_yaw);
            p.set(Param::HeadPitch, head_pitch);
        }
        // Gaze is relative to the head: solve it for the head's actual pose (roll included).
        let [gaze_yaw, gaze_pitch] = self.rig.gaze_toward(&self.params, target);
        let p = &mut self.params;
        let eye_yaw = ease(
            p.get(Param::EyeYaw),
            gaze_yaw.clamp(-EYE_MAX[0], EYE_MAX[0]),
            18.0,
        );
        let eye_pitch = ease(
            p.get(Param::EyePitch),
            gaze_pitch.clamp(-EYE_MAX[1], EYE_MAX[1]),
            18.0,
        );
        p.set(Param::EyeYaw, eye_yaw);
        p.set(Param::EyePitch, eye_pitch);
    }
}

/// The world point under `cursor` (pixels), on a plane facing the camera halfway between it and
/// the orbit target: far enough that the face only turns a little to follow.
fn cursor_target(cam: &Camera3D, cursor: [f32; 2], size: [f32; 2]) -> [f32; 3] {
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let norm = |a: [f32; 3]| {
        let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        [a[0] / l, a[1] / l, a[2] / l]
    };
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let to_target = sub(cam.target, cam.eye);
    let dist = (to_target[0].powi(2) + to_target[1].powi(2) + to_target[2].powi(2)).sqrt();
    let f = norm(to_target);
    let right = norm(cross(f, cam.up));
    let up = cross(right, f);
    let t = (cam.fov_y * 0.5).tan();
    let nx = (cursor[0] / size[0] * 2.0 - 1.0) * t * cam.aspect;
    let ny = (1.0 - cursor[1] / size[1] * 2.0) * t;
    // The ray eye + s·(f + nx·right + ny·up) reaches depth s along f.
    let s = 0.5 * dist;
    std::array::from_fn(|k| cam.eye[k] + s * (f[k] + nx * right[k] + ny * up[k]))
}

/// World point to painter pixels, or `None` behind the camera.
fn project(m: &Mat4, p: [f32; 3], w: f32, h: f32) -> Option<[f32; 2]> {
    let clip: [f32; 4] =
        std::array::from_fn(|r| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r]);
    (clip[3] > 1e-4).then(|| {
        [
            (clip[0] / clip[3] * 0.5 + 0.5) * w,
            (0.5 - clip[1] / clip[3] * 0.5) * h,
        ]
    })
}

fn main() -> Result<()> {
    env_logger::init();
    run::<App>(AppConfig {
        title: "facerig viewer".into(),
        width: 1280,
        height: 800,
        depth_format: Some(wgpu::TextureFormat::Depth32Float),
        msaa_samples: 4,
        ..Default::default()
    })
}
